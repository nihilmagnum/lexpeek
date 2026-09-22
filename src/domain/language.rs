use std::borrow::Cow;

use anyhow::{Context, Result, bail, ensure};

/// Язык, опознаётся по коду ISO 639-1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    pub code: Cow<'static, str>,
    pub name: Cow<'static, str>,
}

impl Language {
    const fn built_in(code: &'static str, name: &'static str) -> Self {
        Self { code: Cow::Borrowed(code), name: Cow::Borrowed(name) }
    }

    /// Язык, добавленный пользователем.
    pub fn custom(code: &str, name: &str) -> Result<Self> {
        let code = code.trim().to_lowercase();
        let name = name.trim();
        ensure!(
            (2..=8).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "language code must be 2 to 8 latin letters"
        );
        ensure!(!name.is_empty(), "language name is empty");
        Ok(Self { code: Cow::Owned(code), name: Cow::Owned(name.to_owned()) })
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

/// Встроенные языки и добавленные пользователем.
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

    pub fn by_code(&self, code: &str) -> Option<&Language> {
        self.languages().find(|language| language.code == code)
    }

    pub fn is_built_in(language: &Language) -> bool {
        BUILT_IN_LANGUAGES.iter().any(|built_in| built_in.code == language.code)
    }

    fn add(&mut self, language: Language) -> Result<()> {
        ensure!(self.by_code(&language.code).is_none(), "language {} is already in the catalog", language.code);
        self.custom.push(language);
        Ok(())
    }

    fn remove(&mut self, language: &Language) {
        self.custom.retain(|custom| custom != language);
    }
}

/// Пара языков пользователя: текст на своём языке переводится на второй, любой другой на свой.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePair {
    pub native: Language,
    pub foreign: Language,
}

impl LanguagePair {
    pub fn new(native: Language, foreign: Language) -> Result<Self> {
        ensure!(native != foreign, "languages of a pair must differ");
        Ok(Self { native, foreign })
    }

    /// Язык перевода для текста на языке `source`.
    pub fn target_for(&self, source: Option<&str>) -> &Language {
        if source == Some(&*self.native.code) { &self.foreign } else { &self.native }
    }

    pub fn languages(&self) -> [Language; 2] {
        [self.native.clone(), self.foreign.clone()]
    }

    pub fn contains(&self, language: &Language) -> bool {
        self.native == *language || self.foreign == *language
    }
}

impl Default for LanguagePair {
    fn default() -> Self {
        Self { native: BUILT_IN_LANGUAGES[0].clone(), foreign: BUILT_IN_LANGUAGES[1].clone() }
    }
}

/// Языки пользователя: каталог и выбранная в нём пара.
#[derive(Clone, Debug, Default)]
pub struct LanguageSetup {
    catalog: LanguageCatalog,
    pair: LanguagePair,
}

impl LanguageSetup {
    /// Восстанавливает сохранённые языки; пара, которой больше нет в каталоге, сбрасывается на пару по умолчанию.
    pub fn restore(custom: Vec<Language>, native: Option<&str>, foreign: Option<&str>) -> Self {
        let mut catalog = LanguageCatalog::default();
        for language in custom {
            let _ = catalog.add(language);
        }
        let find = |code: Option<&str>| code.and_then(|code| catalog.by_code(code)).cloned();
        let pair = find(native)
            .zip(find(foreign))
            .and_then(|(native, foreign)| LanguagePair::new(native, foreign).ok())
            .unwrap_or_default();
        Self { catalog, pair }
    }

    pub fn catalog(&self) -> &LanguageCatalog {
        &self.catalog
    }

    pub fn pair(&self) -> &LanguagePair {
        &self.pair
    }

    pub fn choose(&mut self, native: &str, foreign: &str) -> Result<()> {
        let native = self.catalog.by_code(native).with_context(|| format!("unknown language {native}"))?;
        let foreign = self.catalog.by_code(foreign).with_context(|| format!("unknown language {foreign}"))?;
        self.pair = LanguagePair::new(native.clone(), foreign.clone())?;
        Ok(())
    }

    pub fn add(&mut self, language: Language) -> Result<()> {
        self.catalog.add(language)
    }

    /// Язык из пары удалить нельзя; встроенный остаётся в каталоге.
    pub fn remove(&mut self, language: &Language) -> Result<()> {
        if self.pair.contains(language) {
            bail!("language {} is chosen for translation", language.code);
        }
        self.catalog.remove(language);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_native_text_into_foreign_language_and_any_other_into_native() {
        let pair = LanguagePair::default();
        assert_eq!(pair.target_for(Some("ru")), &pair.foreign);
        assert_eq!(pair.target_for(Some("de")), &pair.native);
        assert_eq!(pair.target_for(None), &pair.native);
    }

    #[test]
    fn keeps_languages_of_the_pair() {
        let mut setup = LanguageSetup::default();
        let georgian = Language::custom("ka", "Грузинский").unwrap();
        setup.add(georgian.clone()).unwrap();
        setup.choose("ru", "ka").unwrap();

        assert!(setup.remove(&georgian).is_err());
        assert!(setup.add(georgian).is_err());
        assert!(setup.choose("ru", "ru").is_err());
    }

    #[test]
    fn restores_default_pair_when_its_language_is_gone() {
        let setup = LanguageSetup::restore(Vec::new(), Some("ru"), Some("ka"));
        assert_eq!(setup.pair(), &LanguagePair::default());
    }
}
