use std::sync::Arc;

use anyhow::Result;
use image::RgbaImage;

use crate::application::languages::LanguageSetupRepository;
use crate::domain::language::{Language, LanguagePair};
use crate::domain::lookup::{Lookup, TermMatch};
use crate::domain::text::SourceText;

pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, fragment: &RgbaImage, languages: &[Language]) -> Result<String>;
}

pub struct Translated {
    pub text: String,
    /// Код языка оригинала, если переводчик его определил.
    pub source: Option<String>,
}

pub trait Translator: Send + Sync {
    /// Переводит на `target`, язык оригинала определяет сам.
    fn translate(&self, text: &SourceText, target: &Language) -> Result<Translated>;
}

/// Распознанный под выделением текст, его перевод и словарные статьи.
pub struct Reading {
    pub text: String,
    pub translation: Result<String>,
    pub terms: Result<Vec<TermMatch>>,
}

/// Читает фрагмент экрана: распознаёт текст, переводит его в паре языков пользователя и ищет в словарях.
pub struct ScreenReading {
    setups: Arc<dyn LanguageSetupRepository>,
    recognizer: Arc<dyn TextRecognizer>,
    translator: Arc<dyn Translator>,
    lookup: Lookup,
}

impl ScreenReading {
    pub fn new(
        setups: Arc<dyn LanguageSetupRepository>,
        recognizer: Arc<dyn TextRecognizer>,
        translator: Arc<dyn Translator>,
        lookup: Lookup,
    ) -> Self {
        Self { setups, recognizer, translator, lookup }
    }

    pub fn read(&self, fragment: &RgbaImage) -> Result<Reading> {
        let pair = self.setups.load()?.pair().clone();
        let text = self.recognizer.recognize(fragment, &pair.languages())?;
        if text.is_empty() {
            return Ok(Reading { text, translation: Ok(String::new()), terms: Ok(Vec::new()) });
        }
        let translation = self.translate(&SourceText::from_recognized(&text), &pair);
        let terms = self.lookup.terms(&text);
        Ok(Reading { text, translation, terms })
    }

    /// Язык оригинала становится известен только после первого перевода, поэтому текст на своём
    /// языке переводится повторно.
    fn translate(&self, text: &SourceText, pair: &LanguagePair) -> Result<String> {
        let translated = self.translator.translate(text, &pair.native)?;
        let target = pair.target_for(translated.source.as_deref());
        if *target == pair.native {
            return Ok(translated.text);
        }
        Ok(self.translator.translate(text, target)?.text)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::domain::dictionary::{Article, HeadwordKey};
    use crate::domain::language::LanguageSetup;
    use crate::domain::lookup::Dictionaries;

    struct DefaultSetups;

    impl LanguageSetupRepository for DefaultSetups {
        fn load(&self) -> Result<LanguageSetup> {
            Ok(LanguageSetup::default())
        }

        fn save(&self, _setup: &LanguageSetup) -> Result<()> {
            Ok(())
        }
    }

    struct FixedText(&'static str);

    impl TextRecognizer for FixedText {
        fn recognize(&self, _fragment: &RgbaImage, _languages: &[Language]) -> Result<String> {
            Ok(self.0.to_owned())
        }
    }

    /// Переводчик, который считает любой текст написанным на заданном языке.
    struct FixedSourceTranslator {
        source: &'static str,
        targets: Mutex<Vec<String>>,
    }

    impl Translator for FixedSourceTranslator {
        fn translate(&self, _text: &SourceText, target: &Language) -> Result<Translated> {
            self.targets.lock().unwrap().push(target.code.to_string());
            Ok(Translated { text: target.code.to_string(), source: Some(self.source.to_owned()) })
        }
    }

    struct NoDictionaries;

    impl Dictionaries for NoDictionaries {
        fn articles(&self, _key: &HeadwordKey) -> Result<Vec<Article>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn translates_native_text_into_foreign_language() {
        let translator = Arc::new(FixedSourceTranslator { source: "ru", targets: Mutex::default() });
        let reading = ScreenReading::new(
            Arc::new(DefaultSetups),
            Arc::new(FixedText("Дверь заперта")),
            translator.clone(),
            Lookup::new(Arc::new(NoDictionaries)),
        );

        let result = reading.read(&RgbaImage::new(1, 1)).unwrap();
        assert_eq!(result.translation.unwrap(), "en");
        assert_eq!(*translator.targets.lock().unwrap(), ["ru", "en"]);
    }
}
