use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::application::translators::{DetectedLanguage, Translated, TranslationRequest, Translator};
use crate::domain::dictionary::{Definition, DictionaryArticle, Meaning, MeaningTranslation};
use crate::domain::language::{Language, LanguageCode};

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    sentences: Vec<Sentence>,
    src: Option<String>,
    ld_result: Option<LanguageDetection>,
    #[serde(default)]
    dict: Vec<DictionaryBlock>,
    #[serde(default)]
    definitions: Vec<DefinitionBlock>,
    #[serde(default)]
    synsets: Vec<SynonymBlock>,
    examples: Option<ExampleBlock>,
}

/// Part of the translation or, without `trans`, transliteration of the original and the translation.
#[derive(Deserialize)]
struct Sentence {
    trans: Option<String>,
    src_translit: Option<String>,
}

#[derive(Deserialize)]
struct LanguageDetection {
    #[serde(default)]
    srclangs_confidences: Vec<f64>,
}

#[derive(Deserialize)]
struct DictionaryBlock {
    pos: String,
    #[serde(default)]
    entry: Vec<DictionaryEntry>,
}

#[derive(Deserialize)]
struct DictionaryEntry {
    word: String,
    #[serde(default)]
    reverse_translation: Vec<String>,
}

#[derive(Deserialize)]
struct DefinitionBlock {
    pos: String,
    #[serde(default)]
    entry: Vec<DefinitionEntry>,
}

#[derive(Deserialize)]
struct DefinitionEntry {
    gloss: String,
    definition_id: Option<String>,
    example: Option<String>,
}

#[derive(Deserialize)]
struct SynonymBlock {
    #[serde(default)]
    entry: Vec<SynonymEntry>,
}

#[derive(Deserialize)]
struct SynonymEntry {
    #[serde(default)]
    synonym: Vec<String>,
    definition_id: Option<String>,
}

#[derive(Deserialize)]
struct ExampleBlock {
    #[serde(default)]
    example: Vec<Example>,
}

#[derive(Deserialize)]
struct Example {
    text: String,
}

/// Google Translate through the endpoint of the Chrome dictionary extension.
pub struct GoogleTranslator {
    agent: ureq::Agent,
}

impl GoogleTranslator {
    const ENDPOINT: &str = "https://clients5.google.com/translate_a/single";
    /// Translation, dictionary, definitions, examples, synonyms and transliteration.
    const DATA_TYPES: [&str; 6] = ["t", "bd", "md", "ex", "ss", "rm"];

    pub fn new() -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build();
        Self { agent: config.into() }
    }

    fn code(language: &Language) -> &str {
        match language.code().as_str() {
            "zh" => "zh-CN",
            code => code,
        }
    }

    /// Language code without the region: zh-CN becomes zh.
    fn language_code(code: &str) -> Option<LanguageCode> {
        LanguageCode::parse(code.split('-').next().unwrap_or(code)).ok()
    }

    fn translated(response: Response) -> Translated {
        let confidence =
            response.ld_result.as_ref().and_then(|detection| detection.srclangs_confidences.first().copied());
        let source = response
            .src
            .as_deref()
            .and_then(Self::language_code)
            .map(|code| DetectedLanguage { code, confidence });
        let text = response.sentences.iter().filter_map(|sentence| sentence.trans.as_deref()).collect();
        Translated { text, source, article: Self::article(response) }
    }

    fn article(response: Response) -> DictionaryArticle {
        let transcription = response.sentences.iter().find_map(|sentence| sentence.src_translit.clone());
        let meanings = response
            .dict
            .into_iter()
            .map(|block| Meaning {
                part_of_speech: block.pos,
                translations: block
                    .entry
                    .into_iter()
                    .map(|entry| MeaningTranslation {
                        word: entry.word,
                        synonyms: Vec::new(),
                        back_translations: entry.reverse_translation,
                    })
                    .collect(),
            })
            .collect();
        let mut synonyms: HashMap<String, Vec<String>> = HashMap::new();
        for entry in response.synsets.into_iter().flat_map(|block| block.entry) {
            if let Some(id) = entry.definition_id {
                synonyms.entry(id).or_default().extend(entry.synonym);
            }
        }
        let mut definitions = Vec::new();
        for block in response.definitions {
            for entry in block.entry {
                definitions.push(Definition {
                    part_of_speech: block.pos.clone(),
                    gloss: entry.gloss,
                    example: entry.example,
                    synonyms: entry.definition_id.and_then(|id| synonyms.remove(&id)).unwrap_or_default(),
                });
            }
        }
        let examples = response
            .examples
            .map(|block| block.example.into_iter().map(|example| Self::plain_text(&example.text)).collect())
            .unwrap_or_default();
        DictionaryArticle { transcription, meanings, definitions, examples }
    }

    /// In examples the word is marked with `<b>` tags.
    fn plain_text(marked: &str) -> String {
        marked.replace("<b>", "").replace("</b>", "")
    }
}

impl Translator for GoogleTranslator {
    fn id(&self) -> &'static str {
        "google"
    }

    fn name(&self) -> &'static str {
        "Google Переводчик"
    }

    /// The `hl` parameter sets the language of the labels and also biases language detection, so while the
    /// language of the original is unknown it carries the likely language of the text.
    fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
        let (source, interface) = match request.source {
            Some(source) => (source.as_str(), Self::code(request.interface)),
            None => ("auto", Self::code(request.likely_source)),
        };
        let parameters = [
            ("client", "dict-chrome-ex"),
            ("sl", source),
            ("tl", Self::code(request.target)),
            ("hl", interface),
            ("dj", "1"),
        ];
        let data_types = Self::DATA_TYPES.map(|data_type| ("dt", data_type));
        let response: Response = self
            .agent
            .post(Self::ENDPOINT)
            .query_pairs(parameters.into_iter().chain(data_types))
            .send_form([("q", request.text.as_str())])
            .context("Google Translate request failed")?
            .body_mut()
            .read_json()
            .context("Google Translate returned malformed response")?;
        Ok(Self::translated(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_full_dictionary_article() {
        let response: Response = serde_json::from_str(include_str!("google_response.json")).unwrap();
        let translated = GoogleTranslator::translated(response);
        let article = &translated.article;

        assert_eq!(translated.text, "перерыв");
        assert_eq!(translated.source.unwrap().code.as_str(), "en");
        assert_eq!(article.transcription.as_deref(), Some("brāk"));
        let first = &article.meanings[0].translations[0];
        assert_eq!(first.word, "перерыв");
        assert!(first.back_translations.contains(&"interruption".to_owned()));
        let shatter = article.definitions.iter().find(|definition| definition.gloss.starts_with("separate")).unwrap();
        assert_eq!(shatter.example.as_deref(), Some("the branch broke with a loud snap"));
        assert!(shatter.synonyms.contains(&"shatter".to_owned()));
        assert!(!article.examples[0].contains("<b>"));
    }
}
