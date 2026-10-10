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

/// Language lexpeek knows out of the box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltInLanguage(LanguageCode);

impl BuiltInLanguage {
    pub fn code(&self) -> &LanguageCode {
        &self.0
    }
}

/// Language added by the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomLanguage {
    code: LanguageCode,
    name: String,
}

impl CustomLanguage {
    pub fn new(code: LanguageCode, name: &str) -> Result<Self, LanguageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LanguageError::EmptyName);
        }
        Ok(Self { code, name: name.to_owned() })
    }

    pub fn code(&self) -> &LanguageCode {
        &self.code
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Language {
    BuiltIn(BuiltInLanguage),
    Custom(CustomLanguage),
}

impl Language {
    const fn built_in(code: &'static str) -> Self {
        Self::BuiltIn(BuiltInLanguage(LanguageCode::built_in(code)))
    }

    pub fn code(&self) -> &LanguageCode {
        match self {
            Language::BuiltIn(language) => language.code(),
            Language::Custom(language) => language.code(),
        }
    }

    pub fn is_built_in(&self) -> bool {
        matches!(self, Language::BuiltIn(_))
    }
}

static BUILT_IN_LANGUAGES: [Language; 13] = [
    Language::built_in("ru"),
    Language::built_in("en"),
    Language::built_in("de"),
    Language::built_in("fr"),
    Language::built_in("es"),
    Language::built_in("it"),
    Language::built_in("pt"),
    Language::built_in("uk"),
    Language::built_in("pl"),
    Language::built_in("tr"),
    Language::built_in("ja"),
    Language::built_in("zh"),
    Language::built_in("ko"),
];

/// Built-in languages and those added by the user.
#[derive(Clone, Debug, Default)]
pub struct LanguageCatalog {
    custom: Vec<CustomLanguage>,
}

impl LanguageCatalog {
    pub fn languages(&self) -> impl Iterator<Item = Language> + '_ {
        BUILT_IN_LANGUAGES.iter().cloned().chain(self.custom.iter().cloned().map(Language::Custom))
    }

    pub fn custom(&self) -> &[CustomLanguage] {
        &self.custom
    }

    pub fn by_code(&self, code: &LanguageCode) -> Option<Language> {
        self.languages().find(|language| language.code() == code)
    }

    fn add(&mut self, language: CustomLanguage) -> Result<(), LanguageError> {
        if self.by_code(language.code()).is_some() {
            return Err(LanguageError::AlreadyInCatalog(language.code));
        }
        self.custom.push(language);
        Ok(())
    }

    fn remove(&mut self, language: &Language) {
        self.custom.retain(|custom| custom.code() != language.code());
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

    /// Pair with another native language; when it is the foreign one, the languages swap.
    pub fn with_native(&self, native: Language) -> Self {
        let foreign = if native == self.foreign { self.native.clone() } else { self.foreign.clone() };
        Self { native, foreign }
    }

    /// Pair with another foreign language; when it is the native one, the languages swap.
    pub fn with_foreign(&self, foreign: Language) -> Self {
        let native = if foreign == self.native { self.foreign.clone() } else { self.native.clone() };
        Self { native, foreign }
    }

    /// Text in the native language is translated into the foreign one, text in any other language into the native one.
    pub fn target_for(&self, source: Option<&LanguageCode>) -> &Language {
        if source == Some(self.native.code()) { &self.foreign } else { &self.native }
    }

    pub fn languages(&self) -> [Language; 2] {
        [self.native.clone(), self.foreign.clone()]
    }

    pub fn contains(&self, language: &Language) -> bool {
        self.native == *language || self.foreign == *language
    }

    pub fn contains_code(&self, code: &LanguageCode) -> bool {
        self.native.code() == code || self.foreign.code() == code
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
        custom: Vec<CustomLanguage>,
        native: Option<LanguageCode>,
        foreign: Option<LanguageCode>,
    ) -> Result<Self, LanguageError> {
        let mut catalog = LanguageCatalog::default();
        for language in custom {
            catalog.add(language)?;
        }
        let find = |code: Option<LanguageCode>| code.and_then(|code| catalog.by_code(&code));
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
        self.pair = LanguagePair::new(native, foreign)?;
        Ok(())
    }

    pub fn add(&mut self, language: CustomLanguage) -> Result<(), LanguageError> {
        self.catalog.add(language)
    }

    /// A language of the pair cannot be removed; a built-in one stays in the catalog.
    pub fn remove(&mut self, language: &Language) -> Result<(), LanguageError> {
        if self.pair.contains(language) {
            return Err(LanguageError::ChosenForTranslation(language.code().clone()));
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
    fn swaps_languages_when_one_takes_the_place_of_the_other() {
        let pair = LanguagePair::default();
        let swapped = pair.with_native(pair.foreign().clone());

        assert_eq!((swapped.native(), swapped.foreign()), (pair.foreign(), pair.native()));
        assert_eq!(pair.with_foreign(pair.native().clone()), swapped);
    }

    #[test]
    fn keeps_languages_of_the_pair() {
        let mut setup = LanguageSetup::default();
        let georgian = CustomLanguage::new(code("ka"), "Georgian").unwrap();
        setup.add(georgian.clone()).unwrap();
        setup.choose(&code("ru"), &code("ka")).unwrap();

        let chosen = Language::Custom(georgian.clone());
        assert_eq!(setup.remove(&chosen), Err(LanguageError::ChosenForTranslation(code("ka"))));
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
