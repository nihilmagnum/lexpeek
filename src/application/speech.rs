use anyhow::Result;

use crate::domain::text::Phrase;

pub trait Speaker: Send + Sync {
    /// Speaks the phrase and returns when it has finished playing.
    fn speak(&self, phrase: &Phrase) -> Result<()>;
}
