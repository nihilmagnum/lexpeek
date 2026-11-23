use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::application::interface::InterfaceLanguageSettings;
use crate::application::languages::LanguageSetupRepository;
use crate::application::translators::TranslatorSettings;
use crate::domain::language::{CustomLanguage, LanguageCode, LanguageSetup};
use crate::infrastructure::atomic_file;

#[derive(Serialize, Deserialize)]
struct StoredLanguage {
    code: String,
    name: String,
}

#[derive(Serialize, Deserialize, Default)]
struct StoredSettings {
    native_language: Option<String>,
    foreign_language: Option<String>,
    #[serde(default)]
    custom_languages: Vec<StoredLanguage>,
    translator: Option<String>,
    interface_language: Option<String>,
}

/// lexpeek settings in a JSON file.
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn stored(&self) -> Result<StoredSettings> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).with_context(|| format!("cannot parse {}", self.path.display())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(StoredSettings::default()),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", self.path.display())),
        }
    }

    fn store(&self, stored: &StoredSettings) -> Result<()> {
        let text = serde_json::to_string_pretty(stored)?;
        atomic_file::replace_file(&self.path, |partial| fs::write(partial, text))
    }
}

impl LanguageSetupRepository for SettingsFile {
    fn load(&self) -> Result<LanguageSetup> {
        let stored = self.stored()?;
        let invalid = || format!("{} has an invalid language", self.path.display());
        let mut custom = Vec::new();
        for language in &stored.custom_languages {
            let code = LanguageCode::parse(&language.code).with_context(invalid)?;
            custom.push(CustomLanguage::new(code, &language.name).with_context(invalid)?);
        }
        let code = |code: &Option<String>| code.as_deref().map(LanguageCode::parse).transpose();
        let native = code(&stored.native_language).with_context(invalid)?;
        let foreign = code(&stored.foreign_language).with_context(invalid)?;
        Ok(LanguageSetup::restore(custom, native, foreign).with_context(invalid)?)
    }

    fn save(&self, setup: &LanguageSetup) -> Result<()> {
        let mut stored = self.stored()?;
        stored.native_language = Some(setup.pair().native().code().to_string());
        stored.foreign_language = Some(setup.pair().foreign().code().to_string());
        stored.custom_languages = setup
            .catalog()
            .custom()
            .iter()
            .map(|language| StoredLanguage { code: language.code().to_string(), name: language.name().to_owned() })
            .collect();
        self.store(&stored)
    }
}

impl TranslatorSettings for SettingsFile {
    fn translator(&self) -> Result<Option<String>> {
        Ok(self.stored()?.translator)
    }

    fn set_translator(&self, id: &str) -> Result<()> {
        let mut stored = self.stored()?;
        stored.translator = Some(id.to_owned());
        self.store(&stored)
    }
}

impl InterfaceLanguageSettings for SettingsFile {
    fn interface_language(&self) -> Result<Option<LanguageCode>> {
        let code = self.stored()?.interface_language;
        let invalid = || format!("{} has an invalid interface language", self.path.display());
        Ok(code.as_deref().map(LanguageCode::parse).transpose().with_context(invalid)?)
    }

    fn set_interface_language(&self, code: Option<&LanguageCode>) -> Result<()> {
        let mut stored = self.stored()?;
        stored.interface_language = code.map(LanguageCode::to_string);
        self.store(&stored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_pair_with_custom_language_translator_and_interface_language() {
        let directory = tempfile::tempdir().unwrap();
        let settings = SettingsFile::new(directory.path().join("settings.json"));
        let mut setup = settings.load().unwrap();
        let georgian = LanguageCode::parse("ka").unwrap();
        setup.add(CustomLanguage::new(georgian.clone(), "Georgian").unwrap()).unwrap();
        setup.choose(&LanguageCode::parse("ru").unwrap(), &georgian).unwrap();

        settings.set_translator("yandex").unwrap();
        settings.set_interface_language(Some(&LanguageCode::parse("en").unwrap())).unwrap();
        settings.save(&setup).unwrap();
        assert_eq!(settings.load().unwrap().pair(), setup.pair());
        assert_eq!(settings.translator().unwrap().as_deref(), Some("yandex"));
        assert_eq!(settings.interface_language().unwrap(), LanguageCode::parse("en").ok());
    }
}
