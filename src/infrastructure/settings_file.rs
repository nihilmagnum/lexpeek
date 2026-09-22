use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::application::languages::LanguageSetupRepository;
use crate::domain::language::{Language, LanguageSetup};

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
}

/// Настройки lexpeek в JSON-файле.
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LanguageSetupRepository for SettingsFile {
    fn load(&self) -> Result<LanguageSetup> {
        let stored: StoredSettings = match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).with_context(|| format!("cannot parse {}", self.path.display()))?,
            Err(error) if error.kind() == ErrorKind::NotFound => StoredSettings::default(),
            Err(error) => return Err(error).with_context(|| format!("cannot read {}", self.path.display())),
        };
        let custom = stored
            .custom_languages
            .iter()
            .filter_map(|language| Language::custom(&language.code, &language.name).ok())
            .collect();
        Ok(LanguageSetup::restore(custom, stored.native_language.as_deref(), stored.foreign_language.as_deref()))
    }

    fn save(&self, setup: &LanguageSetup) -> Result<()> {
        let stored = StoredSettings {
            native_language: Some(setup.pair().native.code.to_string()),
            foreign_language: Some(setup.pair().foreign.code.to_string()),
            custom_languages: setup
                .catalog()
                .custom()
                .iter()
                .map(|language| StoredLanguage { code: language.code.to_string(), name: language.name.to_string() })
                .collect(),
        };
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory).with_context(|| format!("cannot create {}", directory.display()))?;
        }
        let text = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.path, text).with_context(|| format!("cannot write {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_pair_with_custom_language() {
        let directory = tempfile::tempdir().unwrap();
        let settings = SettingsFile::new(directory.path().join("settings.json"));
        let mut setup = settings.load().unwrap();
        setup.add(Language::custom("ka", "Грузинский").unwrap()).unwrap();
        setup.choose("ru", "ka").unwrap();

        settings.save(&setup).unwrap();
        assert_eq!(settings.load().unwrap().pair(), setup.pair());
    }
}
