use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::translators::{
    BatchTranslationRequest, DetectedLanguage, Translated, TranslatedText, TranslationRequest, Translator,
};
use crate::domain::dictionary::{DictionaryArticle, Meaning, MeaningTranslation};
use crate::domain::language::LanguageCode;
use crate::domain::text::SourceText;

#[derive(Deserialize)]
struct Detection {
    code: u16,
    lang: Option<String>,
}

#[derive(Deserialize)]
struct Translation {
    code: u16,
    /// Direction of the translation, for example `en-ru`.
    lang: Option<String>,
    #[serde(default)]
    text: Vec<String>,
}

impl Translation {
    fn source(&self) -> Option<LanguageCode> {
        let direction = self.lang.as_deref()?;
        LanguageCode::parse(direction.split('-').next().unwrap_or(direction)).ok()
    }
}

#[derive(Deserialize)]
struct DictionaryEntry {
    pos: Option<PartOfSpeech>,
    ts: Option<String>,
    #[serde(default)]
    tr: Vec<DictionaryTranslation>,
}

#[derive(Deserialize)]
struct PartOfSpeech {
    tooltip: Option<String>,
    text: String,
}

#[derive(Deserialize)]
struct DictionaryTranslation {
    text: String,
    #[serde(default)]
    syn: Vec<Word>,
    #[serde(default)]
    mean: Vec<Word>,
}

#[derive(Deserialize)]
struct Word {
    text: String,
}

/// Yandex Translate and Yandex Dictionary through the mobile app endpoint.
pub struct YandexTranslator {
    agent: ureq::Agent,
    /// Client identifier the endpoint requires in every request.
    client_id: String,
}

impl YandexTranslator {
    const TRANSLATE_ENDPOINT: &str = "https://translate.yandex.net/api/v1/tr.json";
    const DICTIONARY_ENDPOINT: &str = "https://dictionary.yandex.net/dicservice.json/lookupMultiple";
    const SUCCESS: u16 = 200;
    /// Yandex rejects requests with more characters of text.
    const REQUEST_LIMIT: usize = 10_000;

    pub fn new() -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build();
        Self { agent: config.into(), client_id: format!("{}-0-0", Uuid::new_v4().simple()) }
    }

    /// Yandex detects the language more reliably among hinted languages than without a hint.
    fn detect(&self, request: &TranslationRequest) -> Result<LanguageCode> {
        let hint = format!("{},{}", request.likely_source.code(), request.target.code());
        let detection: Detection = self
            .agent
            .get(format!("{}/detect", Self::TRANSLATE_ENDPOINT))
            .query_pairs([("id", self.client_id.as_str()), ("srv", "android"), ("text", request.text.as_str())])
            .query("hint", &hint)
            .call()
            .context("Yandex Translate detection failed")?
            .body_mut()
            .read_json()
            .context("Yandex Translate returned malformed detection")?;
        ensure!(detection.code == Self::SUCCESS, "Yandex Translate detection failed with code {}", detection.code);
        let lang = detection.lang.filter(|lang| !lang.is_empty());
        Ok(LanguageCode::parse(&lang.context("Yandex Translate could not detect the language")?)?)
    }

    fn translate_text(&self, request: &TranslationRequest, source: &LanguageCode) -> Result<String> {
        let direction = format!("{source}-{}", request.target.code());
        Ok(self.fetch(std::slice::from_ref(request.text), &direction)?.text.join("\n"))
    }

    /// Yandex detects one language for all texts of a request. When most of them are already in the target
    /// language, the others are left untranslated, so they are translated again from the likely language.
    fn translate_chunk(&self, texts: &[SourceText], request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
        let (target, likely) = (request.target.code(), request.likely_source.code());
        let mut translation = self.fetch(texts, target.as_str())?;
        if translation.source().as_ref() == Some(target) && likely != target {
            translation = self.fetch(texts, &format!("{likely}-{target}"))?;
        }
        let (received, expected) = (translation.text.len(), texts.len());
        ensure!(received == expected, "Yandex Translate returned {received} of {expected} texts");
        let source = translation.source();
        Ok(translation.text.into_iter().map(|text| TranslatedText { text, source: source.clone() }).collect())
    }

    /// `direction` is either the target language, then Yandex detects the source, or both, like `en-ru`.
    fn fetch(&self, texts: &[SourceText], direction: &str) -> Result<Translation> {
        let form = texts.iter().map(|text| ("text", text.as_str())).chain([("lang", direction)]);
        let translation: Translation = self
            .agent
            .post(format!("{}/translate", Self::TRANSLATE_ENDPOINT))
            .query_pairs([("id", self.client_id.as_str()), ("srv", "android")])
            .send_form(form)
            .context("Yandex Translate request failed")?
            .body_mut()
            .read_json()
            .context("Yandex Translate returned malformed response")?;
        ensure!(translation.code == Self::SUCCESS, "Yandex Translate failed with code {}", translation.code);
        Ok(translation)
    }

    /// Consecutive texts within the request limit; a longer text goes alone.
    fn chunks(texts: &[SourceText]) -> Vec<&[SourceText]> {
        let mut chunks = Vec::new();
        let (mut start, mut length) = (0, 0);
        for (index, text) in texts.iter().enumerate() {
            let text_length = text.as_str().chars().count();
            if index > start && length + text_length > Self::REQUEST_LIMIT {
                chunks.push(&texts[start..index]);
                (start, length) = (index, 0);
            }
            length += text_length;
        }
        chunks.push(&texts[start..]);
        chunks
    }

    /// Only words and short phrases have an article; the translation is useful without it,
    /// so a dictionary failure does not interrupt the translation.
    fn article(
        &self,
        request: &TranslationRequest,
        source: &LanguageCode,
        labels_language: &LanguageCode,
    ) -> DictionaryArticle {
        match self.lookup(request, source, labels_language) {
            Ok(article) => article,
            Err(error) => {
                eprintln!("lexpeek: Yandex Dictionary is unavailable: {error:#}");
                DictionaryArticle::default()
            }
        }
    }

    fn lookup(
        &self,
        request: &TranslationRequest,
        source: &LanguageCode,
        labels_language: &LanguageCode,
    ) -> Result<DictionaryArticle> {
        let direction = format!("{source}-{}", request.target.code());
        let response: serde_json::Value = self
            .agent
            .get(Self::DICTIONARY_ENDPOINT)
            .query_pairs([
                ("ui", labels_language.as_str()),
                ("srv", "tr-text"),
                ("text", request.text.as_str()),
                ("dict", direction.as_str()),
            ])
            .call()?
            .body_mut()
            .read_json()?;
        let Some(entries) = response.get(&direction).and_then(|entries| entries.get("regular")) else {
            return Ok(DictionaryArticle::default());
        };
        let entries: Vec<DictionaryEntry> = serde_json::from_value(entries.clone())?;
        Ok(Self::dictionary_article(entries))
    }

    fn dictionary_article(entries: Vec<DictionaryEntry>) -> DictionaryArticle {
        let transcription = entries.iter().find_map(|entry| entry.ts.clone());
        let meanings = entries
            .into_iter()
            .map(|entry| Meaning {
                part_of_speech: entry.pos.map(|pos| pos.tooltip.unwrap_or(pos.text)).unwrap_or_default(),
                translations: entry
                    .tr
                    .into_iter()
                    .map(|translation| MeaningTranslation {
                        word: translation.text,
                        synonyms: translation.syn.into_iter().map(|word| word.text).collect(),
                        back_translations: translation.mean.into_iter().map(|word| word.text).collect(),
                    })
                    .collect(),
            })
            .collect();
        DictionaryArticle { transcription, meanings, definitions: Vec::new(), examples: Vec::new() }
    }
}

impl Translator for YandexTranslator {
    fn id(&self) -> &'static str {
        "yandex"
    }

    fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
        let source = match request.source {
            Some(source) => source.clone(),
            None => self.detect(request)?,
        };
        let text = self.translate_text(request, &source)?;
        let article = match request.labels_language {
            Some(labels_language) => self.article(request, &source, labels_language),
            None => DictionaryArticle::default(),
        };
        // Yandex does not report detection confidence.
        Ok(Translated { text, source: Some(DetectedLanguage { code: source, confidence: None }), article })
    }

    fn translate_batch(&self, request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
        let mut translated = Vec::with_capacity(request.texts.len());
        for chunk in Self::chunks(request.texts) {
            translated.extend(self.translate_chunk(chunk, request)?);
        }
        Ok(translated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dictionary_article() {
        let response: serde_json::Value = serde_json::from_str(include_str!("yandex_dictionary.json")).unwrap();
        let entries: Vec<DictionaryEntry> = serde_json::from_value(response["en-ru"]["regular"].clone()).unwrap();
        let article = YandexTranslator::dictionary_article(entries);

        assert_eq!(article.transcription.as_deref(), Some("breɪk"));
        let first = &article.meanings[0];
        assert_eq!(first.part_of_speech, "существительное");
        assert_eq!(first.translations[0].word, "перерыв");
        assert!(first.translations[0].synonyms.contains(&"брейк".to_owned()));
    }
}
