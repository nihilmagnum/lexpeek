use std::sync::Arc;

use anyhow::Result;

use crate::domain::text::Phrase;

pub trait Speaker: Send + Sync {
    /// Произносит фразу и возвращается, когда она прозвучала.
    fn speak(&self, phrase: &Phrase) -> Result<()>;
}

/// Озвучивание оригинала и перевода.
pub struct ReadingSpeech {
    speaker: Arc<dyn Speaker>,
}

impl ReadingSpeech {
    pub fn new(speaker: Arc<dyn Speaker>) -> Self {
        Self { speaker }
    }

    pub fn speak(&self, phrase: &Phrase) -> Result<()> {
        self.speaker.speak(phrase)
    }
}
