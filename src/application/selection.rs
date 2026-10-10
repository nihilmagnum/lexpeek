use std::sync::Arc;

use anyhow::{Context, Result};

use crate::application::reading::{Reading, TextReading};
use crate::domain::language::LanguageCode;

/// Text selected in any window.
pub trait SelectedText: Send + Sync {
    fn read(&self) -> Result<Option<String>>;
}

pub trait Clipboard: Send + Sync {
    fn put(&self, text: &str) -> Result<()>;
}

/// Paste keystroke in the active window.
pub trait PasteKeystroke: Send + Sync {
    fn paste(&self) -> Result<()>;
}

/// Translation of the selected text: show it, copy it to the clipboard or paste it over the selection.
pub struct SelectionTranslation {
    selection: Arc<dyn SelectedText>,
    reading: Arc<TextReading>,
    clipboard: Arc<dyn Clipboard>,
    keystroke: Arc<dyn PasteKeystroke>,
}

impl SelectionTranslation {
    pub fn new(
        selection: Arc<dyn SelectedText>,
        reading: Arc<TextReading>,
        clipboard: Arc<dyn Clipboard>,
        keystroke: Arc<dyn PasteKeystroke>,
    ) -> Self {
        Self { selection, reading, clipboard, keystroke }
    }

    /// `None` if nothing is selected.
    pub fn selected_text(&self) -> Result<Option<String>> {
        Ok(self.selection.read()?.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty()))
    }

    /// Translation of the selected text with a dictionary article.
    pub fn read(&self, text: &str, labels_language: &LanguageCode) -> Result<Reading> {
        self.reading.read_text(text, labels_language)
    }

    pub fn copy(&self) -> Result<()> {
        let text = self.selected_text()?.context("no text is selected")?;
        self.clipboard.put(&self.reading.translate(&text)?)
    }

    /// The selection stays in the active window, so pasting the translation replaces it.
    pub fn replace(&self) -> Result<()> {
        self.copy()?;
        self.keystroke.paste()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use image::RgbaImage;

    use super::*;
    use crate::application::languages::LanguageSetupRepository;
    use crate::application::reading::TextRecognizer;
    use crate::application::translators::{
        BatchTranslationRequest, DetectedLanguage, Translated, TranslatedText, TranslationRequest, Translator,
        TranslatorSettings, Translators,
    };
    use crate::domain::dictionary::DictionaryArticle;
    use crate::domain::language::{Language, LanguageCode, LanguageSetup};

    struct FixedSelection(Option<&'static str>);

    impl SelectedText for FixedSelection {
        fn read(&self) -> Result<Option<String>> {
            Ok(self.0.map(str::to_owned))
        }
    }

    #[derive(Default)]
    struct InMemoryClipboard(Mutex<Option<String>>);

    impl Clipboard for InMemoryClipboard {
        fn put(&self, text: &str) -> Result<()> {
            *self.0.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct CountingKeystroke(AtomicUsize);

    impl PasteKeystroke for CountingKeystroke {
        fn paste(&self) -> Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Translates English text into Russian, appending a mark to it.
    struct MarkingTranslator;

    impl Translator for MarkingTranslator {
        fn id(&self) -> &'static str {
            "marking"
        }

        fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
            Ok(Translated {
                text: format!("{} ({})", request.text.as_str(), request.target.code()),
                source: Some(DetectedLanguage { code: LanguageCode::parse("en")?, confidence: None }),
                article: DictionaryArticle::default(),
            })
        }

        fn translate_batch(&self, _request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
            unreachable!("selected text is translated as a whole")
        }
    }

    struct DefaultSetups;

    impl LanguageSetupRepository for DefaultSetups {
        fn load(&self) -> Result<LanguageSetup> {
            Ok(LanguageSetup::default())
        }

        fn save(&self, _setup: &LanguageSetup) -> Result<()> {
            Ok(())
        }
    }

    struct NoRecognizer;

    impl TextRecognizer for NoRecognizer {
        fn recognize(&self, _fragment: &RgbaImage, _languages: &[Language]) -> Result<String> {
            Ok(String::new())
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

    struct Fixture {
        translation: SelectionTranslation,
        clipboard: Arc<InMemoryClipboard>,
        keystroke: Arc<CountingKeystroke>,
    }

    fn fixture(selected: Option<&'static str>) -> Fixture {
        let translators = Arc::new(Translators::new(vec![Arc::new(MarkingTranslator)], Arc::new(NoChoice)));
        let reading = Arc::new(TextReading::new(Arc::new(DefaultSetups), Arc::new(NoRecognizer), translators));
        let clipboard = Arc::new(InMemoryClipboard::default());
        let keystroke = Arc::new(CountingKeystroke::default());
        let selection = Arc::new(FixedSelection(selected));
        let translation = SelectionTranslation::new(selection, reading, clipboard.clone(), keystroke.clone());
        Fixture { translation, clipboard, keystroke }
    }

    #[test]
    fn copies_translation_and_pastes_it_only_when_replacing() {
        let fixture = fixture(Some(" door "));

        fixture.translation.copy().unwrap();
        assert_eq!(fixture.clipboard.0.lock().unwrap().as_deref(), Some("door (ru)"));
        assert_eq!(fixture.keystroke.0.load(Ordering::SeqCst), 0);

        fixture.translation.replace().unwrap();
        assert_eq!(fixture.keystroke.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn does_nothing_without_selection() {
        let fixture = fixture(None);

        assert!(fixture.translation.selected_text().unwrap().is_none());
        assert!(fixture.translation.replace().is_err());
        assert!(fixture.clipboard.0.lock().unwrap().is_none());
        assert_eq!(fixture.keystroke.0.load(Ordering::SeqCst), 0);
    }
}
