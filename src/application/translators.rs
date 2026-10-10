use std::sync::Arc;

use anyhow::{Context, Result, ensure};

use crate::domain::dictionary::DictionaryArticle;
use crate::domain::language::{Language, LanguageCode};
use crate::domain::text::SourceText;

pub struct TranslationRequest<'a> {
    pub text: &'a SourceText,
    /// Language of the original; without it the translator detects the language itself.
    pub source: Option<&'a LanguageCode>,
    pub target: &'a Language,
    /// Most likely language of the text: helps to detect the language of a short text.
    pub likely_source: &'a Language,
    /// Language of the part-of-speech labels and other notes in the dictionary article.
    pub interface: &'a Language,
}

/// Language of the original as detected by the translator.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedLanguage {
    pub code: LanguageCode,
    /// Confidence from 0 to 1, if the translator reports it.
    pub confidence: Option<f64>,
}

pub struct Translated {
    pub text: String,
    pub source: Option<DetectedLanguage>,
    /// Present only for words and short phrases.
    pub article: DictionaryArticle,
}

pub trait Translator: Send + Sync {
    /// Stable identifier under which the choice of translator is stored in the settings.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn translate(&self, request: &TranslationRequest) -> Result<Translated>;
}

pub trait TranslatorSettings: Send + Sync {
    /// Identifier of the chosen translator, if the user has chosen one.
    fn translator(&self) -> Result<Option<String>>;
    fn set_translator(&self, id: &str) -> Result<()>;
}

/// Translator in the choice list.
pub struct TranslatorOption {
    pub id: &'static str,
    pub name: &'static str,
}

/// Available translators and the one chosen by the user.
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

    /// Until the user chooses a translator, or if the chosen one is gone, the first in the list is used.
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
