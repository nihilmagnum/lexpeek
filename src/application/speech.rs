use std::sync::Arc;

use anyhow::Result;

use crate::domain::text::Phrase;

pub trait Speaker: Send + Sync {
    /// Speaks the phrase and returns when it has finished playing.
    fn speak(&self, phrase: &Phrase) -> Result<()>;
}

/// Speech for the original and the translation.
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
