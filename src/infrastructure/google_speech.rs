use std::time::Duration;

use anyhow::{Context, Result};

use crate::application::speech::Speaker;
use crate::domain::language::LanguageCode;
use crate::domain::text::Phrase;
use crate::infrastructure::audio_player::GtkAudioPlayer;
use crate::infrastructure::{google, http};

/// Text-to-speech of Google Translate.
pub struct GoogleSpeaker {
    agent: ureq::Agent,
    player: GtkAudioPlayer,
}

impl GoogleSpeaker {
    const ENDPOINT: &str = "https://clients5.google.com/translate_tts";
    /// Google does not speak anything longer than this in one request.
    const MAX_CHUNK: usize = 200;

    pub fn new() -> Self {
        Self { agent: http::agent(Duration::from_secs(15)), player: GtkAudioPlayer }
    }

    fn synthesize(&self, chunk: &str, language: &LanguageCode) -> Result<Vec<u8>> {
        self.agent
            .get(Self::ENDPOINT)
            .query_pairs([("client", "dict-chrome-ex"), ("ie", "UTF-8"), ("tl", google::code(language)), ("q", chunk)])
            .call()
            .context("Google speech request failed")?
            .body_mut()
            .read_to_vec()
            .context("Google speech returned no audio")
    }

    /// Chunks no longer than `MAX_CHUNK`, split at sentences, and long sentences at words.
    fn chunks(text: &str) -> Vec<String> {
        let mut chunks: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in text.split_whitespace() {
            let fits = current.chars().count() + 1 + word.chars().count() <= Self::MAX_CHUNK;
            if !current.is_empty() && !fits {
                chunks.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            if word.ends_with(['.', '!', '?', '…']) && current.chars().count() > Self::MAX_CHUNK / 2 {
                chunks.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }
}

impl Speaker for GoogleSpeaker {
    /// MP3 frames are independent, so the audio of the chunks is joined into one recording that plays without pauses.
    fn speak(&self, phrase: &Phrase) -> Result<()> {
        let mut audio = Vec::new();
        for chunk in Self::chunks(&phrase.text) {
            audio.extend(self.synthesize(&chunk, &phrase.language)?);
        }
        self.player.play(audio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_long_text_into_short_chunks_without_breaking_words() {
        let text = "Дверь заперта. ".repeat(30);
        let chunks = GoogleSpeaker::chunks(&text);

        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= GoogleSpeaker::MAX_CHUNK));
        assert_eq!(chunks.join(" "), text.trim());
    }
}
