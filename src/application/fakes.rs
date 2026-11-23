//! Test doubles of the ports that tests of several services need.

use anyhow::Result;
use image::RgbaImage;

use crate::application::languages::LanguageSetupRepository;
use crate::application::reading::TextRecognizer;
use crate::application::translators::TranslatorSettings;
use crate::domain::language::{Language, LanguageSetup};

/// Default language setup that is never saved.
pub struct DefaultSetups;

impl LanguageSetupRepository for DefaultSetups {
    fn load(&self) -> Result<LanguageSetup> {
        Ok(LanguageSetup::default())
    }

    fn save(&self, _setup: &LanguageSetup) -> Result<()> {
        Ok(())
    }
}

/// Recognizer that finds no text.
pub struct NoText;

impl TextRecognizer for NoText {
    fn recognize(&self, _fragment: &RgbaImage, _languages: &[Language]) -> Result<String> {
        Ok(String::new())
    }
}

/// Settings in which no translator is chosen.
pub struct NoChoice;

impl TranslatorSettings for NoChoice {
    fn translator(&self) -> Result<Option<String>> {
        Ok(None)
    }

    fn set_translator(&self, _id: &str) -> Result<()> {
        Ok(())
    }
}
