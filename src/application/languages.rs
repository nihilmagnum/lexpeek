use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use thiserror::Error;

use crate::domain::language::{CustomLanguage, Language, LanguageCode, LanguageError, LanguagePair, LanguageSetup};

#[derive(Debug, Error)]
pub enum LanguagesError {
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error("OCR model for {0} is not installed")]
    ModelMissing(LanguageCode),
    #[error("language {0} has no model to download")]
    NotDownloadable(LanguageCode),
    /// Settings or models could not be read, written or downloaded.
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

pub trait LanguageSetupRepository: Send + Sync {
    fn load(&self) -> Result<LanguageSetup>;
    fn save(&self, setup: &LanguageSetup) -> Result<()>;
}

/// Text recognition models per language.
pub trait OcrModelStore: Send + Sync {
    fn is_installed(&self, language: &Language) -> bool;
    /// Downloads the model of a built-in language.
    fn download(&self, language: &Language) -> Result<()>;
    fn install_from_file(&self, language: &Language, file: &Path) -> Result<()>;
    fn remove(&self, language: &Language) -> Result<()>;
}

pub struct LanguageStatus {
    pub language: Language,
    pub installed: bool,
    pub in_pair: bool,
}

pub struct LanguagesOverview {
    pub pair: LanguagePair,
    pub languages: Vec<LanguageStatus>,
}

/// Choice of the language pair and management of recognition models.
pub struct LanguagesService {
    language_setups: Arc<dyn LanguageSetupRepository>,
    models: Arc<dyn OcrModelStore>,
}

impl LanguagesService {
    pub fn new(language_setups: Arc<dyn LanguageSetupRepository>, models: Arc<dyn OcrModelStore>) -> Self {
        Self { language_setups, models }
    }

    pub fn overview(&self) -> Result<LanguagesOverview> {
        let setup = self.language_setups.load()?;
        let languages = setup
            .catalog()
            .languages()
            .map(|language| LanguageStatus {
                installed: self.models.is_installed(&language),
                in_pair: setup.pair().contains(&language),
                language,
            })
            .collect();
        Ok(LanguagesOverview { pair: setup.pair().clone(), languages })
    }

    /// Only languages that can be recognized are suitable for the pair.
    pub fn choose(&self, native: &LanguageCode, foreign: &LanguageCode) -> Result<(), LanguagesError> {
        let mut setup = self.language_setups.load()?;
        setup.choose(native, foreign)?;
        for language in setup.pair().languages() {
            if !self.models.is_installed(&language) {
                return Err(LanguagesError::ModelMissing(language.code().clone()));
            }
        }
        Ok(self.language_setups.save(&setup)?)
    }

    pub fn download(&self, language: &Language) -> Result<(), LanguagesError> {
        if !language.is_built_in() {
            return Err(LanguagesError::NotDownloadable(language.code().clone()));
        }
        Ok(self.models.download(language)?)
    }

    /// A built-in language loses only its model, a user-added one leaves the catalog.
    pub fn remove(&self, language: &Language) -> Result<(), LanguagesError> {
        let mut setup = self.language_setups.load()?;
        setup.remove(language)?;
        if self.models.is_installed(language) {
            self.models.remove(language)?;
        }
        Ok(self.language_setups.save(&setup)?)
    }

    /// A model for the code of a built-in language becomes its model, otherwise a new language joins the catalog.
    pub fn add_from_file(&self, file: &Path, code: &LanguageCode, name: &str) -> Result<(), LanguagesError> {
        let mut setup = self.language_setups.load()?;
        if let Some(known) = setup.catalog().by_code(code) {
            if !known.is_built_in() {
                return Err(LanguageError::AlreadyInCatalog(code.clone()).into());
            }
            return Ok(self.models.install_from_file(&known, file)?);
        }
        let language = CustomLanguage::new(code.clone(), name)?;
        setup.add(language.clone())?;
        self.models.install_from_file(&Language::Custom(language), file)?;
        Ok(self.language_setups.save(&setup)?)
    }

    /// Downloads missing models of the pair, for example on first launch.
    pub fn download_pair_models(&self) -> Result<()> {
        for language in self.language_setups.load()?.pair().languages() {
            if language.is_built_in() && !self.models.is_installed(&language) {
                self.models.download(&language)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct InMemorySetups(Mutex<LanguageSetup>);

    impl LanguageSetupRepository for InMemorySetups {
        fn load(&self) -> Result<LanguageSetup> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, setup: &LanguageSetup) -> Result<()> {
            *self.0.lock().unwrap() = setup.clone();
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryModels(Mutex<HashSet<String>>);

    impl OcrModelStore for InMemoryModels {
        fn is_installed(&self, language: &Language) -> bool {
            self.0.lock().unwrap().contains(language.code().as_str())
        }

        fn download(&self, language: &Language) -> Result<()> {
            self.0.lock().unwrap().insert(language.code().to_string());
            Ok(())
        }

        fn install_from_file(&self, language: &Language, _file: &Path) -> Result<()> {
            self.download(language)
        }

        fn remove(&self, language: &Language) -> Result<()> {
            self.0.lock().unwrap().remove(language.code().as_str());
            Ok(())
        }
    }

    fn code(code: &str) -> LanguageCode {
        LanguageCode::parse(code).unwrap()
    }

    fn service() -> LanguagesService {
        LanguagesService::new(Arc::new(InMemorySetups::default()), Arc::new(InMemoryModels::default()))
    }

    #[test]
    fn custom_language_from_file_joins_the_catalog_and_leaves_it_on_removal() {
        let languages = service();
        languages.download_pair_models().unwrap();
        languages.add_from_file(Path::new("kat.traineddata"), &code("ka"), "Georgian").unwrap();
        languages.choose(&code("ru"), &code("ka")).unwrap();
        let georgian = languages.overview().unwrap().pair.foreign().clone();

        let error = languages.remove(&georgian).unwrap_err();
        assert!(matches!(error, LanguagesError::Language(LanguageError::ChosenForTranslation(c)) if c == code("ka")));
        languages.choose(&code("ru"), &code("en")).unwrap();
        languages.remove(&georgian).unwrap();
        assert!(languages.overview().unwrap().languages.iter().all(|status| status.language != georgian));
    }

    #[test]
    fn pair_needs_installed_models() {
        let languages = service();
        languages.download_pair_models().unwrap();
        let error = languages.choose(&code("ru"), &code("de")).unwrap_err();
        assert!(matches!(error, LanguagesError::ModelMissing(c) if c == code("de")));
    }
}
