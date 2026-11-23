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

/// Translation of a text together with its dictionary article.
#[derive(Clone)]
pub struct Translation {
    pub text: String,
    /// Language of the translation.
    pub language: Language,
    /// Language of the original, if the translator named it.
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

/// Recognized or selected text and its translation.
pub struct Reading {
    pub text: String,
    pub translation: Result<Translation>,
}

impl Reading {
    /// The language of the original is known only from a successful translation.
    pub fn original(&self) -> Option<Phrase> {
        let language = self.translation.as_ref().ok()?.source.clone()?;
        Some(Phrase { text: SourceText::from_recognized(&self.text).as_str().to_owned(), language })
    }
}

/// Reads text from the screen and translates it within the user's language pair.
pub struct TextReading {
    language_setups: Arc<dyn LanguageSetupRepository>,
    recognizer: Arc<dyn TextRecognizer>,
    translators: Arc<Translators>,
}

impl TextReading {
    /// Below this confidence the language of the text is clarified with the foreign language as a hint.
    const CONFIDENT_DETECTION: f64 = 0.9;

    pub fn new(
        language_setups: Arc<dyn LanguageSetupRepository>,
        recognizer: Arc<dyn TextRecognizer>,
        translators: Arc<Translators>,
    ) -> Self {
        Self { language_setups, recognizer, translators }
    }

    /// Recognizes text in a fragment of a screenshot and translates it; `None` if the fragment has no text.
    pub fn read_fragment(&self, fragment: &RgbaImage, labels_language: &LanguageCode) -> Result<Option<Reading>> {
        let pair = self.language_setups.load()?.pair().clone();
        let text = self.recognizer.recognize(fragment, &pair.languages())?;
        if text.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.read(text, &pair, labels_language)))
    }

    pub fn read_text(&self, text: &str, labels_language: &LanguageCode) -> Result<Reading> {
        let pair = self.language_setups.load()?.pair().clone();
        Ok(self.read(text.trim().to_owned(), &pair, labels_language))
    }

    /// Only the translated text, without the dictionary article.
    pub fn translate(&self, text: &str) -> Result<String> {
        let pair = self.language_setups.load()?.pair().clone();
        Ok(self.translate_in_pair(&SourceText::from_recognized(text), &pair, None)?.text)
    }

    fn read(&self, text: String, pair: &LanguagePair, labels_language: &LanguageCode) -> Reading {
        let translation = self.translate_in_pair(&SourceText::from_recognized(&text), pair, Some(labels_language));
        Reading { text, translation }
    }

    /// The language of the original becomes known only after the first translation, so text in the native
    /// language or with a clarified language is translated again.
    fn translate_in_pair(
        &self,
        text: &SourceText,
        pair: &LanguagePair,
        labels_language: Option<&LanguageCode>,
    ) -> Result<Translation> {
        let translator = self.translators.selected()?;
        let native = pair.native();
        let to_native =
            TranslationRequest { text, source: None, target: native, likely_source: native, labels_language };
        let translated = translator.translate(&to_native)?;
        let detected = translated.source.as_ref().map(|source| source.code.clone());
        let source = Self::clarify_source(translator.as_ref(), text, pair, translated.source.as_ref())?;
        let target = pair.target_for(source.as_ref());
        if target == native && source == detected {
            return Ok(Translation::new(translated, target, source));
        }
        let request =
            TranslationRequest { text, source: source.as_ref(), target, likely_source: native, labels_language };
        Ok(Translation::new(translator.translate(&request)?, target, source))
    }

    /// Translators often confuse the language of a single word with a similar one, such as Russian with Bulgarian.
    /// If unsure and the language is outside the pair, asks again with the foreign language as the likely one.
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
            labels_language: None,
        };
        let clarified = translator.translate(&hinted)?.source.map(|source| source.code);
        Ok(Some(clarified.filter(|code| pair.contains_code(code)).unwrap_or_else(|| detected.code.clone())))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::application::fakes::{DefaultSetups, NoChoice, NoText};
    use crate::application::translators::{BatchTranslationRequest, TranslatedText};

    /// Translator that recognizes `language` only with the `recognized_with` hint and otherwise
    /// takes it for Bulgarian. The translation is the code of the target language.
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

        fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
            let source = request.source.map_or("auto", LanguageCode::as_str);
            let (target, likely) = (request.target.code(), request.likely_source.code());
            let labels = request.labels_language.map_or("none", LanguageCode::as_str);
            self.requests.lock().unwrap().push(format!("{source}->{target} likely {likely} labels {labels}"));
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

        fn translate_batch(&self, _request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
            unreachable!("text is read one piece at a time")
        }
    }

    fn reading(translator: Arc<HintSensitiveTranslator>) -> TextReading {
        let translators = Translators::new(vec![translator], Arc::new(NoChoice));
        TextReading::new(Arc::new(DefaultSetups), Arc::new(NoText), Arc::new(translators))
    }

    #[test]
    fn translates_native_text_into_foreign_language() {
        let translator = HintSensitiveTranslator::new("ru", "ru");

        assert_eq!(reading(translator.clone()).translate("версия").unwrap(), "en");
        let requests = translator.requests.lock().unwrap();
        assert_eq!(*requests, ["auto->ru likely ru labels none", "ru->en likely ru labels none"]);
    }

    #[test]
    fn clarifies_ambiguous_word_by_foreign_language() {
        let translator = HintSensitiveTranslator::new("en", "en");

        let labels_language = LanguageCode::parse("en").unwrap();
        let reading = reading(translator.clone()).read_text("version", &labels_language).unwrap();
        assert_eq!(reading.translation.unwrap().text, "ru");
        let requests = translator.requests.lock().unwrap();
        let expected = ["auto->ru likely ru labels en", "auto->ru likely en labels none", "en->ru likely ru labels en"];
        assert_eq!(*requests, expected);
    }

    #[test]
    fn translates_nothing_when_fragment_has_no_text() {
        let translator = HintSensitiveTranslator::new("en", "en");

        let labels_language = LanguageCode::parse("en").unwrap();
        let reading = reading(translator.clone()).read_fragment(&RgbaImage::new(1, 1), &labels_language).unwrap();
        assert!(reading.is_none());
        assert!(translator.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn names_language_of_original() {
        let translator = HintSensitiveTranslator::new("en", "en");

        let labels_language = LanguageCode::parse("ru").unwrap();
        let original = reading(translator).read_text("version", &labels_language).unwrap().original();
        let expected = Phrase { text: "version".to_owned(), language: LanguageCode::parse("en").unwrap() };
        assert_eq!(original, Some(expected));
    }
}
