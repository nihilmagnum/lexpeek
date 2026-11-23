use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::application::translators::{
    BatchTranslationRequest, DetectedLanguage, Translated, TranslatedText, TranslationRequest, Translator,
};
use crate::domain::dictionary::{Definition, DictionaryArticle, Meaning, MeaningTranslation};
use crate::domain::language::LanguageCode;
use crate::infrastructure::http;

/// Code of the language in Google services, which know Chinese only by its regional variants.
pub fn code(language: &LanguageCode) -> &str {
    match language.as_str() {
        "zh" => "zh-CN",
        code => code,
    }
}

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

/// Translation of one text of a batch and the detected language of the original.
#[derive(Deserialize)]
struct BatchItem(String, String);

/// Google Translate through the endpoints of the Chrome dictionary extension.
pub struct GoogleTranslator {
    agent: ureq::Agent,
}

impl GoogleTranslator {
    const ENDPOINT: &str = "https://clients5.google.com/translate_a/single";
    const BATCH_ENDPOINT: &str = "https://clients5.google.com/translate_a/t";
    /// Translation, dictionary, definitions, examples, synonyms and transliteration.
    const DATA_TYPES: [&str; 6] = ["t", "bd", "md", "ex", "ss", "rm"];

    pub fn new() -> Self {
        Self { agent: http::agent(Duration::from_secs(10)) }
    }

    fn fetch(&self, request: &TranslationRequest, source: &str, labels_language: &LanguageCode) -> Result<Translated> {
        let parameters = [
            ("client", "dict-chrome-ex"),
            ("sl", source),
            ("tl", code(request.target.code())),
            ("hl", code(labels_language)),
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

    /// The `hl` parameter sets the language of the labels and also biases language detection, so while the
    /// language of the original is unknown it carries the likely language of the text. A dictionary article
    /// labelled in another language then has to be requested again with the detected language of the original.
    fn translate(&self, request: &TranslationRequest) -> Result<Translated> {
        let likely = request.likely_source.code();
        if let Some(source) = request.source {
            return self.fetch(request, source.as_str(), request.labels_language.unwrap_or(likely));
        }
        let detected = self.fetch(request, "auto", likely)?;
        let missing_labels = request.labels_language.filter(|labels| *labels != likely && !detected.article.is_empty());
        let (Some(labels_language), Some(source)) = (missing_labels, &detected.source) else {
            return Ok(detected);
        };
        let labelled = self.fetch(request, source.code.as_str(), labels_language)?;
        Ok(Translated { source: detected.source, ..labelled })
    }

    /// The language of each text is detected separately.
    fn translate_batch(&self, request: &BatchTranslationRequest) -> Result<Vec<TranslatedText>> {
        let parameters = [("client", "dict-chrome-ex"), ("sl", "auto"), ("tl", code(request.target.code()))];
        let items: Vec<BatchItem> = self
            .agent
            .post(Self::BATCH_ENDPOINT)
            .query_pairs(parameters)
            .send_form(request.texts.iter().map(|text| ("q", text.as_str())))
            .context("Google Translate request failed")?
            .body_mut()
            .read_json()
            .context("Google Translate returned malformed response")?;
        let expected = request.texts.len();
        ensure!(items.len() == expected, "Google Translate returned {} of {expected} texts", items.len());
        Ok(items
            .into_iter()
            .map(|BatchItem(text, source)| TranslatedText { text, source: Self::language_code(&source) })
            .collect())
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
