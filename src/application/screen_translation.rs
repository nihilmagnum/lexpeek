use std::sync::Arc;

use anyhow::Result;
use image::RgbaImage;

use crate::application::languages::LanguageSetupRepository;
use crate::application::translators::{BatchTranslationRequest, Translators};
use crate::domain::language::Language;
use crate::domain::text::{ScreenParagraph, SourceText};

pub trait ParagraphRecognizer: Send + Sync {
    /// Paragraphs of text anywhere on the screen.
    fn recognize_paragraphs(&self, screen: &RgbaImage, languages: &[Language]) -> Result<Vec<ScreenParagraph>>;
}

/// Paragraph of the screen and its translation.
pub struct TranslatedParagraph {
    pub paragraph: ScreenParagraph,
    pub translation: String,
}

/// Translates all text on the screen into the native language, the way a browser translates a page.
pub struct ScreenTranslation {
    language_setups: Arc<dyn LanguageSetupRepository>,
    recognizer: Arc<dyn ParagraphRecognizer>,
    translators: Arc<Translators>,
}

impl ScreenTranslation {
    pub fn new(
        language_setups: Arc<dyn LanguageSetupRepository>,
        recognizer: Arc<dyn ParagraphRecognizer>,
        translators: Arc<Translators>,
    ) -> Self {
        Self { language_setups, recognizer, translators }
    }

    /// Paragraphs already in the native language are left out.
    pub fn translate(&self, screen: &RgbaImage) -> Result<Vec<TranslatedParagraph>> {
        let pair = self.language_setups.load()?.pair().clone();
        let paragraphs = self.recognizer.recognize_paragraphs(screen, &pair.languages())?;
        if paragraphs.is_empty() {
            return Ok(Vec::new());
        }
        let texts: Vec<SourceText> =
            paragraphs.iter().map(|paragraph| SourceText::from_recognized(&paragraph.text)).collect();
        let request = BatchTranslationRequest { texts: &texts, target: pair.native(), likely_source: pair.foreign() };
        let translations = self.translators.selected()?.translate_batch(&request)?;
        let native = pair.native().code();
        Ok(paragraphs
            .into_iter()
            .zip(&texts)
            .zip(translations)
            .filter(|((_, text), translated)| {
                translated.source.as_ref() != Some(native) && translated.text.trim() != text.as_str()
            })
            .map(|((paragraph, _), translated)| TranslatedParagraph { paragraph, translation: translated.text })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::translators::{
        Translated, TranslatedText, TranslationRequest, Translator, TranslatorSettings,
    };
    use crate::domain::language::{LanguageCode, LanguageSetup};
    use crate::domain::text::ScreenArea;

    struct DefaultSetups;

    impl LanguageSetupRepository for DefaultSetups {
        fn load(&self) -> Result<LanguageSetup> {
            Ok(LanguageSetup::default())
        }

        fn save(&self, _setup: &LanguageSetup) -> Result<()> {
            Ok(())
        }
    }

    struct Paragraphs(Vec<&'static str>);

    impl ParagraphRecognizer for Paragraphs {
        fn recognize_paragraphs(&self, _screen: &RgbaImage, _languages: &[Language]) -> Result<Vec<ScreenParagraph>> {
            let area = ScreenArea { x: 0, y: 0, width: 100, height: 20 };
            Ok(self.0.iter().map(|text| ScreenParagraph { text: (*text).to_owned(), area, font_size: 20 }).collect())
        }
    }

    /// Knows a few English phrases; text in Latin letters is taken for English and anything else for Russian.
    struct Dictionary;

    impl Translator for Dictionary {
        fn id(&self) -> &'static str {
            "dictionary"
        }

        fn translate(&self, _request: &TranslationRequest) -> Result<Translated> {
            unreachable!("the screen is translated in a batch")
        }

        fn translate_batch(&self, request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
            request
                .texts
                .iter()
                .map(|text| {
                    let english = text.as_str().chars().any(|letter| letter.is_ascii_alphabetic());
                    let translated = match text.as_str() {
                        "Open file" => "Открыть файл",
                        "File" => "Файл",
                        other => other,
                    };
                    let source = LanguageCode::parse(if english { "en" } else { "ru" })?;
                    Ok(TranslatedText { text: translated.to_owned(), source: Some(source) })
                })
                .collect()
        }
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
    fn translates_paragraphs_not_in_native_language() {
        let recognizer = Paragraphs(vec!["Open\nfile", "Файл", "Lexpeek"]);
        let translators = Translators::new(vec![Arc::new(Dictionary)], Arc::new(NoChoice));
        let screen = ScreenTranslation::new(Arc::new(DefaultSetups), Arc::new(recognizer), Arc::new(translators));

        let translated = screen.translate(&RgbaImage::new(1, 1)).unwrap();
        let translations: Vec<&str> = translated.iter().map(|paragraph| paragraph.translation.as_str()).collect();
        assert_eq!(translations, ["Открыть файл"]);
        assert_eq!(translated[0].paragraph.text, "Open\nfile");
    }
}
