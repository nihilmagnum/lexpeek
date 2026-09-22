use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail, ensure};

use crate::domain::language::{Language, LanguageCatalog, LanguagePair, LanguageSetup};

pub trait LanguageSetupRepository: Send + Sync {
    fn load(&self) -> Result<LanguageSetup>;
    fn save(&self, setup: &LanguageSetup) -> Result<()>;
}

/// Модели распознавания текста по языкам.
pub trait OcrModelStore: Send + Sync {
    fn is_installed(&self, language: &Language) -> bool;
    /// Скачивает модель встроенного языка.
    fn download(&self, language: &Language) -> Result<()>;
    fn install_from_file(&self, language: &Language, file: &Path) -> Result<()>;
    fn remove(&self, language: &Language) -> Result<()>;
}

pub struct LanguageStatus {
    pub language: Language,
    pub built_in: bool,
    pub installed: bool,
    pub in_pair: bool,
}

pub struct LanguagesOverview {
    pub pair: LanguagePair,
    pub languages: Vec<LanguageStatus>,
}

/// Выбор пары языков и управление моделями распознавания.
pub struct LanguagesService {
    setups: Arc<dyn LanguageSetupRepository>,
    models: Arc<dyn OcrModelStore>,
}

impl LanguagesService {
    pub fn new(setups: Arc<dyn LanguageSetupRepository>, models: Arc<dyn OcrModelStore>) -> Self {
        Self { setups, models }
    }

    pub fn pair(&self) -> Result<LanguagePair> {
        Ok(self.setups.load()?.pair().clone())
    }

    pub fn overview(&self) -> Result<LanguagesOverview> {
        let setup = self.setups.load()?;
        let languages = setup
            .catalog()
            .languages()
            .map(|language| LanguageStatus {
                language: language.clone(),
                built_in: LanguageCatalog::is_built_in(language),
                installed: self.models.is_installed(language),
                in_pair: setup.pair().contains(language),
            })
            .collect();
        Ok(LanguagesOverview { pair: setup.pair().clone(), languages })
    }

    /// Для пары годятся только языки, которые умеем распознавать.
    pub fn choose(&self, native: &str, foreign: &str) -> Result<()> {
        let mut setup = self.setups.load()?;
        setup.choose(native, foreign)?;
        for language in setup.pair().languages() {
            ensure!(self.models.is_installed(&language), "OCR model for {} is not installed", language.code);
        }
        self.setups.save(&setup)
    }

    pub fn download(&self, language: &Language) -> Result<()> {
        ensure!(LanguageCatalog::is_built_in(language), "language {} has no model to download", language.code);
        self.models.download(language)
    }

    /// Встроенный язык теряет только модель, добавленный пользователем исчезает из каталога.
    pub fn remove(&self, language: &Language) -> Result<()> {
        let mut setup = self.setups.load()?;
        setup.remove(language)?;
        if self.models.is_installed(language) {
            self.models.remove(language)?;
        }
        self.setups.save(&setup)
    }

    /// Модель для кода встроенного языка становится его моделью, иначе в каталоге появляется новый язык.
    pub fn add_from_file(&self, file: &Path, code: &str, name: &str) -> Result<()> {
        let mut setup = self.setups.load()?;
        if let Some(known) = setup.catalog().by_code(&code.trim().to_lowercase()) {
            if !LanguageCatalog::is_built_in(known) {
                bail!("language {} is already added", known.code);
            }
            return self.models.install_from_file(known, file);
        }
        let language = Language::custom(code, name)?;
        setup.add(language.clone())?;
        self.models.install_from_file(&language, file)?;
        self.setups.save(&setup)
    }

    /// Скачивает недостающие модели пары, например при первом запуске.
    pub fn download_pair_models(&self) -> Result<()> {
        for language in self.pair()?.languages() {
            if LanguageCatalog::is_built_in(&language) && !self.models.is_installed(&language) {
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
            self.0.lock().unwrap().contains(&*language.code)
        }

        fn download(&self, language: &Language) -> Result<()> {
            self.0.lock().unwrap().insert(language.code.to_string());
            Ok(())
        }

        fn install_from_file(&self, language: &Language, _file: &Path) -> Result<()> {
            self.download(language)
        }

        fn remove(&self, language: &Language) -> Result<()> {
            self.0.lock().unwrap().remove(&*language.code);
            Ok(())
        }
    }

    fn service() -> LanguagesService {
        LanguagesService::new(Arc::new(InMemorySetups::default()), Arc::new(InMemoryModels::default()))
    }

    #[test]
    fn custom_language_from_file_joins_the_catalog_and_leaves_it_on_removal() {
        let languages = service();
        languages.download_pair_models().unwrap();
        languages.add_from_file(Path::new("kat.traineddata"), "ka", "Грузинский").unwrap();
        languages.choose("ru", "ka").unwrap();
        let georgian = languages.pair().unwrap().foreign;

        assert!(languages.remove(&georgian).is_err());
        languages.choose("ru", "en").unwrap();
        languages.remove(&georgian).unwrap();
        assert!(languages.overview().unwrap().languages.iter().all(|status| status.language != georgian));
    }

    #[test]
    fn pair_needs_installed_models() {
        let languages = service();
        languages.download_pair_models().unwrap();
        assert!(languages.choose("ru", "de").is_err());
    }
}
