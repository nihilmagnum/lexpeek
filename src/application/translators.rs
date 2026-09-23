use std::sync::Arc;

use anyhow::{Context, Result, ensure};

use crate::domain::dictionary::DictionaryArticle;
use crate::domain::language::{Language, LanguageCode};
use crate::domain::text::SourceText;

pub struct TranslationRequest<'a> {
    pub text: &'a SourceText,
    /// Язык оригинала; без него переводчик определяет язык сам.
    pub source: Option<&'a LanguageCode>,
    pub target: &'a Language,
    /// Самый вероятный язык текста: помогает определить язык короткого текста.
    pub likely_source: &'a Language,
    /// Язык, на котором подписаны части речи и другие пояснения словарной статьи.
    pub interface: &'a Language,
}

/// Язык оригинала, как его определил переводчик.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedLanguage {
    pub code: LanguageCode,
    /// Уверенность от 0 до 1, если переводчик её сообщает.
    pub confidence: Option<f64>,
}

pub struct Translated {
    pub text: String,
    pub source: Option<DetectedLanguage>,
    /// Бывает только у слов и коротких выражений.
    pub article: DictionaryArticle,
}

pub trait Translator: Send + Sync {
    /// Постоянный идентификатор, под которым выбор переводчика хранится в настройках.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn translate(&self, request: &TranslationRequest) -> Result<Translated>;
}

pub trait TranslatorSettings: Send + Sync {
    /// Идентификатор выбранного переводчика, если пользователь его выбирал.
    fn translator(&self) -> Result<Option<String>>;
    fn set_translator(&self, id: &str) -> Result<()>;
}

/// Переводчик в списке выбора.
pub struct TranslatorOption {
    pub id: &'static str,
    pub name: &'static str,
}

/// Доступные переводчики и выбранный пользователем.
pub struct Translators {
    available: Vec<Arc<dyn Translator>>,
    settings: Arc<dyn TranslatorSettings>,
}

impl Translators {
    pub fn new(available: Vec<Arc<dyn Translator>>, settings: Arc<dyn TranslatorSettings>) -> Self {
        Self { available, settings }
    }

    pub fn options(&self) -> Vec<TranslatorOption> {
        self.available
            .iter()
            .map(|translator| TranslatorOption { id: translator.id(), name: translator.name() })
            .collect()
    }

    /// Пока пользователь не выбрал переводчик или выбранного больше нет, работает первый в списке.
    pub fn selected(&self) -> Result<Arc<dyn Translator>> {
        let chosen = self.settings.translator()?;
        let selected = chosen.and_then(|id| self.available.iter().find(|translator| translator.id() == id));
        selected.or_else(|| self.available.first()).cloned().context("no translators available")
    }

    pub fn select(&self, id: &str) -> Result<()> {
        ensure!(self.available.iter().any(|translator| translator.id() == id), "unknown translator {id}");
        self.settings.set_translator(id)
    }
}
