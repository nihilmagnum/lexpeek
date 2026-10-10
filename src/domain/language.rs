use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LanguageError {
    #[error("language code {0:?} must be 2 to 8 latin letters")]
    InvalidCode(String),
    #[error("language name is empty")]
    EmptyName,
    #[error("language {0} is already in the catalog")]
    AlreadyInCatalog(LanguageCode),
    #[error("language {0} is not in the catalog")]
    Unknown(LanguageCode),
    #[error("languages of a pair must differ")]
    SameLanguagesInPair,
    #[error("language {0} is chosen for translation")]
    ChosenForTranslation(LanguageCode),
}

/// ISO 639-1 language code, for example `ru`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LanguageCode(Cow<'static, str>);

impl LanguageCode {
    const fn built_in(code: &'static str) -> Self {
        Self(Cow::Borrowed(code))
    }

    /// Allows a language variant after a hyphen, for example `zh-tw`.
    pub fn parse(code: &str) -> Result<Self, LanguageError> {
        let code = code.trim().to_lowercase();
        let valid = (2..=8).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase() || c == '-');
        if !valid {
            return Err(LanguageError::InvalidCode(code));
        }
        Ok(Self(Cow::Owned(code)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    code: LanguageCode,
    name: Cow<'static, str>,
}

impl Language {
    const fn built_in(code: &'static str, name: &'static str) -> Self {
        Self { code: LanguageCode::built_in(code), name: Cow::Borrowed(name) }
    }

    /// Language added by the user.
    pub fn custom(code: LanguageCode, name: &str) -> Result<Self, LanguageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LanguageError::EmptyName);
        }
        Ok(Self { code, name: Cow::Owned(name.to_owned()) })
    }

    pub fn code(&self) -> &LanguageCode {
        &self.code
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

static BUILT_IN_LANGUAGES: [Language; 13] = [
    Language::built_in("ru", "Русский"),
    Language::built_in("en", "Английский"),
    Language::built_in("de", "Немецкий"),
    Language::built_in("fr", "Французский"),
    Language::built_in("es", "Испанский"),
    Language::built_in("it", "Итальянский"),
    Language::built_in("pt", "Португальский"),
    Language::built_in("uk", "Украинский"),
    Language::built_in("pl", "Польский"),
    Language::built_in("tr", "Турецкий"),
    Language::built_in("ja", "Японский"),
    Language::built_in("zh", "Китайский"),
    Language::built_in("ko", "Корейский"),
];

/// Built-in languages and those added by the user.
#[derive(Clone, Debug, Default)]
pub struct LanguageCatalog {
    custom: Vec<Language>,
}

impl LanguageCatalog {
    pub fn languages(&self) -> impl Iterator<Item = &Language> {
        BUILT_IN_LANGUAGES.iter().chain(&self.custom)
    }

    pub fn custom(&self) -> &[Language] {
        &self.custom
    }

    pub fn by_code(&self, code: &LanguageCode) -> Option<&Language> {
        self.languages().find(|language| language.code == *code)
    }

    pub fn is_built_in(language: &Language) -> bool {
        BUILT_IN_LANGUAGES.iter().any(|built_in| built_in.code == language.code)
    }

    fn add(&mut self, language: Language) -> Result<(), LanguageError> {
        if self.by_code(&language.code).is_some() {
            return Err(LanguageError::AlreadyInCatalog(language.code));
        }
        self.custom.push(language);
        Ok(())
    }

    fn remove(&mut self, language: &Language) {
        self.custom.retain(|custom| custom != language);
    }
}

/// The user's language pair: native and foreign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePair {
    native: Language,
    foreign: Language,
}

impl LanguagePair {
    pub fn new(native: Language, foreign: Language) -> Result<Self, LanguageError> {
        if native == foreign {
            return Err(LanguageError::SameLanguagesInPair);
        }
        Ok(Self { native, foreign })
    }

    pub fn native(&self) -> &Language {
        &self.native
    }

    pub fn foreign(&self) -> &Language {
        &self.foreign
    }

    /// Text in the native language is translated into the foreign one, text in any other language into the native one.
    pub fn target_for(&self, source: Option<&LanguageCode>) -> &Language {
        if source == Some(&self.native.code) { &self.foreign } else { &self.native }
    }

    pub fn languages(&self) -> [Language; 2] {
        [self.native.clone(), self.foreign.clone()]
    }

    pub fn contains(&self, language: &Language) -> bool {
        self.native == *language || self.foreign == *language
    }

    pub fn contains_code(&self, code: &LanguageCode) -> bool {
        self.native.code == *code || self.foreign.code == *code
    }
}

impl Default for LanguagePair {
    fn default() -> Self {
        Self { native: BUILT_IN_LANGUAGES[0].clone(), foreign: BUILT_IN_LANGUAGES[1].clone() }
    }
}

/// The user's languages: the catalog and the pair chosen from it.
#[derive(Clone, Debug, Default)]
pub struct LanguageSetup {
    catalog: LanguageCatalog,
    pair: LanguagePair,
}

impl LanguageSetup {
    /// A pair whose language is no longer in the catalog is reset to the default pair.
    pub fn restore(
        custom: Vec<Language>,
        native: Option<LanguageCode>,
        foreign: Option<LanguageCode>,
    ) -> Result<Self, LanguageError> {
        let mut catalog = LanguageCatalog::default();
        for language in custom {
            catalog.add(language)?;
        }
        let find = |code: Option<LanguageCode>| code.and_then(|code| catalog.by_code(&code).cloned());
        let pair = match (find(native), find(foreign)) {
            (Some(native), Some(foreign)) => LanguagePair::new(native, foreign).unwrap_or_default(),
            _ => LanguagePair::default(),
        };
        Ok(Self { catalog, pair })
    }

    pub fn catalog(&self) -> &LanguageCatalog {
        &self.catalog
    }

    pub fn pair(&self) -> &LanguagePair {
        &self.pair
    }

    pub fn choose(&mut self, native: &LanguageCode, foreign: &LanguageCode) -> Result<(), LanguageError> {
        let native = self.catalog.by_code(native).ok_or_else(|| LanguageError::Unknown(native.clone()))?;
        let foreign = self.catalog.by_code(foreign).ok_or_else(|| LanguageError::Unknown(foreign.clone()))?;
        self.pair = LanguagePair::new(native.clone(), foreign.clone())?;
        Ok(())
    }

    pub fn add(&mut self, language: Language) -> Result<(), LanguageError> {
        self.catalog.add(language)
    }

    /// A language of the pair cannot be removed; a built-in one stays in the catalog.
    pub fn remove(&mut self, language: &Language) -> Result<(), LanguageError> {
        if self.pair.contains(language) {
            return Err(LanguageError::ChosenForTranslation(language.code.clone()));
        }
        self.catalog.remove(language);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(code: &str) -> LanguageCode {
        LanguageCode::parse(code).unwrap()
    }

    #[test]
    fn translates_native_text_into_foreign_language_and_any_other_into_native() {
        let pair = LanguagePair::default();
        assert_eq!(pair.target_for(Some(&code("ru"))), pair.foreign());
        assert_eq!(pair.target_for(Some(&code("de"))), pair.native());
        assert_eq!(pair.target_for(None), pair.native());
    }

    #[test]
    fn keeps_languages_of_the_pair() {
        let mut setup = LanguageSetup::default();
        let georgian = Language::custom(code("ka"), "Georgian").unwrap();
        setup.add(georgian.clone()).unwrap();
        setup.choose(&code("ru"), &code("ka")).unwrap();

        assert_eq!(setup.remove(&georgian), Err(LanguageError::ChosenForTranslation(code("ka"))));
        assert_eq!(setup.add(georgian), Err(LanguageError::AlreadyInCatalog(code("ka"))));
        assert_eq!(setup.choose(&code("ru"), &code("ru")), Err(LanguageError::SameLanguagesInPair));
    }

    #[test]
    fn restores_default_pair_when_its_language_is_gone() {
        let setup = LanguageSetup::restore(Vec::new(), Some(code("ru")), Some(code("ka"))).unwrap();
        assert_eq!(setup.pair(), &LanguagePair::default());
    }

    #[test]
    fn rejects_malformed_code() {
        assert!(LanguageCode::parse("r").is_err());
        assert!(LanguageCode::parse("ру").is_err());
        assert_eq!(code(" ZH-tw ").as_str(), "zh-tw");
    }
}
