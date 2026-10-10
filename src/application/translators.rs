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
    /// Language of the part-of-speech labels and other notes in the dictionary article;
    /// without it the article is not needed.
    pub labels_language: Option<&'a LanguageCode>,
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

/// Texts translated each on its own, in as few requests as the translator allows.
pub struct BatchTranslationRequest<'a> {
    pub texts: &'a [SourceText],
    pub target: &'a Language,
    /// Most likely language of the texts that need translation.
    pub likely_source: &'a Language,
}

/// Translation of one text of a batch.
pub struct TranslatedText {
    pub text: String,
    /// Language of the original, if the translator named it.
    pub source: Option<LanguageCode>,
}

pub trait Translator: Send + Sync {
    /// Stable identifier under which the choice of translator is stored in the settings.
    fn id(&self) -> &'static str;
    fn translate(&self, request: &TranslationRequest) -> Result<Translated>;
    /// One translation per text, in the order of the texts.
    fn translate_batch(&self, request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>>;
}

pub trait TranslatorSettings: Send + Sync {
    /// Identifier of the chosen translator, if the user has chosen one.
    fn translator(&self) -> Result<Option<String>>;
    fn set_translator(&self, id: &str) -> Result<()>;
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

    pub fn ids(&self) -> Vec<&'static str> {
        self.available.iter().map(|translator| translator.id()).collect()
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
