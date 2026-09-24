use std::sync::Arc;

use anyhow::Result;

use crate::application::reading::Translation;
use crate::domain::language::Language;

pub trait Speaker: Send + Sync {
    /// Произносит текст и возвращается, когда он прозвучал.
    fn speak(&self, text: &str, language: &Language) -> Result<()>;
}

/// Озвучивание перевода.
pub struct TranslationSpeech {
    speaker: Arc<dyn Speaker>,
}

impl TranslationSpeech {
    pub fn new(speaker: Arc<dyn Speaker>) -> Self {
        Self { speaker }
    }

    pub fn speak(&self, translation: &Translation) -> Result<()> {
        self.speaker.speak(&translation.text, &translation.language)
    }
}
