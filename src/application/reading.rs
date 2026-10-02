use std::sync::Arc;

use anyhow::Result;
use image::RgbaImage;

use crate::application::languages::LanguageSetupRepository;
use crate::application::translators::{DetectedLanguage, Translated, TranslationRequest, Translator, Translators};
use crate::domain::dictionary::DictionaryArticle;
use crate::domain::language::{Language, LanguageCode, LanguagePair};
use crate::domain::text::{Phrase, SourceText};

pub trait TextRecognizer: Send + Sync {
    fn recognize(&self, fragment: &RgbaImage, languages: &[Language]) -> Result<String>;
}

/// Перевод текста вместе со словарной статьёй.
#[derive(Clone)]
pub struct Translation {
    pub text: String,
    /// Язык перевода.
    pub language: Language,
    /// Язык оригинала, если переводчик его назвал.
    pub source: Option<LanguageCode>,
    pub article: DictionaryArticle,
}

impl Translation {
    fn new(translated: Translated, language: &Language, source: Option<LanguageCode>) -> Self {
        Self { text: translated.text, language: language.clone(), source, article: translated.article }
    }

    pub fn phrase(&self) -> Phrase {
        Phrase { text: self.text.clone(), language: self.language.code().clone() }
    }
}

/// Распознанный или выделенный текст и его перевод.
pub struct Reading {
    pub text: String,
    pub translation: Result<Translation>,
}

impl Reading {
    /// Язык оригинала известен только из удавшегося перевода.
    pub fn original(&self) -> Option<Phrase> {
        let language = self.translation.as_ref().ok()?.source.clone()?;
        Some(Phrase { text: SourceText::from_recognized(&self.text).as_str().to_owned(), language })
    }
}

/// Читает текст с экрана и переводит его в паре языков пользователя.
pub struct TextReading {
    language_setups: Arc<dyn LanguageSetupRepository>,
    recognizer: Arc<dyn TextRecognizer>,
    translators: Arc<Translators>,
}

impl TextReading {
    /// Ниже этой уверенности язык текста уточняется по второму языку пары.
    const CONFIDENT_DETECTION: f64 = 0.9;

    pub fn new(
        language_setups: Arc<dyn LanguageSetupRepository>,
        recognizer: Arc<dyn TextRecognizer>,
        translators: Arc<Translators>,
    ) -> Self {
        Self { language_setups, recognizer, translators }
    }

    /// Распознаёт текст на фрагменте снимка экрана и переводит его.
    pub fn read_fragment(&self, fragment: &RgbaImage) -> Result<Reading> {
        let pair = self.language_setups.load()?.pair().clone();
        let text = self.recognizer.recognize(fragment, &pair.languages())?;
        Ok(self.read(text, &pair))
    }

    pub fn read_text(&self, text: &str) -> Result<Reading> {
        let pair = self.language_setups.load()?.pair().clone();
        Ok(self.read(text.trim().to_owned(), &pair))
    }

    /// Только текст перевода, без словарной статьи.
    pub fn translate(&self, text: &str) -> Result<String> {
        let pair = self.language_setups.load()?.pair().clone();
        Ok(self.translate_in_pair(&SourceText::from_recognized(text), &pair)?.text)
    }

    fn read(&self, text: String, pair: &LanguagePair) -> Reading {
        if text.is_empty() {
            let language = pair.native().clone();
            let article = DictionaryArticle::default();
            let translation = Translation { text: String::new(), language, source: None, article };
            return Reading { text, translation: Ok(translation) };
        }
        let translation = self.translate_in_pair(&SourceText::from_recognized(&text), pair);
        Reading { text, translation }
    }

    /// Язык оригинала становится известен только после первого перевода, поэтому текст на своём
    /// языке или с уточнённым языком переводится повторно.
    fn translate_in_pair(&self, text: &SourceText, pair: &LanguagePair) -> Result<Translation> {
        let translator = self.translators.selected()?;
        let native = pair.native();
        let to_native =
            TranslationRequest { text, source: None, target: native, likely_source: native, interface: native };
        let translated = translator.translate(&to_native)?;
        let detected = translated.source.as_ref().map(|source| source.code.clone());
        let source = Self::clarify_source(translator.as_ref(), text, pair, translated.source.as_ref())?;
        let target = pair.target_for(source.as_ref());
        if target == native && source == detected {
            return Ok(Translation::new(translated, target, source));
        }
        let request =
            TranslationRequest { text, source: source.as_ref(), target, likely_source: native, interface: native };
        Ok(Translation::new(translator.translate(&request)?, target, source))
    }

    /// Язык отдельного слова переводчик часто путает с похожим, например русский с болгарским.
    /// Если он не уверен и назвал язык вне пары, переспрашивает, считая вероятным второй язык.
    fn clarify_source(
        translator: &dyn Translator,
        text: &SourceText,
        pair: &LanguagePair,
        detected: Option<&DetectedLanguage>,
    ) -> Result<Option<LanguageCode>> {
        let Some(detected) = detected else {
            return Ok(None);
        };
        let confident = detected.confidence.is_none_or(|confidence| confidence >= Self::CONFIDENT_DETECTION);
        if pair.contains_code(&detected.code) || confident {
            return Ok(Some(detected.code.clone()));
        }
        let hinted = TranslationRequest {
            text,
            source: None,
            target: pair.native(),
            likely_source: pair.foreign(),
            interface: pair.native(),
        };
        let clarified = translator.translate(&hinted)?.source.map(|source| source.code);
        Ok(Some(clarified.filter(|code| pair.contains_code(code)).unwrap_or_else(|| detected.code.clone())))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::application::translators::TranslatorSettings;
    use crate::domain::language::LanguageSetup;

    struct DefaultSetups;

    impl LanguageSetupRepository for DefaultSetups {
        fn load(&self) -> Result<LanguageSetup> {
            Ok(LanguageSetup::default())
        }

        fn save(&self, _setup: &LanguageSetup) -> Result<()> {
            Ok(())
        }
    }

    struct NoText;

    impl TextRecognizer for NoText {
        fn recognize(&self, _fragment: &RgbaImage, _languages: &[Language]) -> Result<String> {
            Ok(String::new())
        }
    }

    /// Переводчик, который узнаёт `language` только с подсказкой `recognized_with`, а иначе
    /// принимает его за болгарский. Переводом служит код целевого языка.
    struct HintSensitiveTranslator {
        language: &'static str,
        recognized_with: &'static str,
        requests: Mutex<Vec<String>>,
    }

    impl HintSensitiveTranslator {
        fn new(language: &'static str, recognized_with: &'static str) -> Arc<Self> {
            Arc::new(Self { language, recognized_with, requests: Mutex::default() })
        }
    }

    impl Translator for HintSensitiveTranslator {
        fn id(&self) -> &'static str {
            "hint-sensitive"
        }

        fn name(&self) -> &'static str {
            "Test"
        }

        fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
            let source = request.source.map_or("auto", LanguageCode::as_str);
            let (target, likely) = (request.target.code(), request.likely_source.code());
            self.requests.lock().unwrap().push(format!("{source}->{target} likely {likely}"));
            let detected = match request.source {
                Some(code) => DetectedLanguage { code: code.clone(), confidence: None },
                None if likely.as_str() == self.recognized_with => {
                    DetectedLanguage { code: LanguageCode::parse(self.language)?, confidence: Some(0.99) }
                }
                None => DetectedLanguage { code: LanguageCode::parse("bg")?, confidence: Some(0.6) },
            };
            let text = target.to_string();
            Ok(Translated { text, source: Some(detected), article: DictionaryArticle::default() })
        }
    }

    fn reading(translator: Arc<HintSensitiveTranslator>) -> TextReading {
        let translators = Translators::new(vec![translator], Arc::new(NoChoice));
        TextReading::new(Arc::new(DefaultSetups), Arc::new(NoText), Arc::new(translators))
    }

    struct NoChoice;

    impl TranslatorSettings for NoChoice {
        fn translator(&self) -> Result<Option<String>> {
            Ok(None)
        }

        fn set_translator(&self, _id: &str) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn translates_native_text_into_foreign_language() {
        let translator = HintSensitiveTranslator::new("ru", "ru");

        assert_eq!(reading(translator.clone()).translate("версия").unwrap(), "en");
        assert_eq!(*translator.requests.lock().unwrap(), ["auto->ru likely ru", "ru->en likely ru"]);
    }

    #[test]
    fn clarifies_ambiguous_word_by_foreign_language() {
        let translator = HintSensitiveTranslator::new("en", "en");

        assert_eq!(reading(translator.clone()).translate("version").unwrap(), "ru");
        let requests = translator.requests.lock().unwrap();
        assert_eq!(*requests, ["auto->ru likely ru", "auto->ru likely en", "en->ru likely ru"]);
    }

    #[test]
    fn names_language_of_original() {
        let translator = HintSensitiveTranslator::new("en", "en");

        let original = reading(translator).read_text("version").unwrap().original();
        let expected = Phrase { text: "version".to_owned(), language: LanguageCode::parse("en").unwrap() };
        assert_eq!(original, Some(expected));
    }
}
