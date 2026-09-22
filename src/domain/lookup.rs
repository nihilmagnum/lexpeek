use std::sync::Arc;

use anyhow::Result;

use crate::domain::dictionary::{Article, HeadwordKey};
use crate::domain::english;

pub trait Dictionaries: Send + Sync {
    fn articles(&self, key: &HeadwordKey) -> Result<Vec<Article>>;
}

/// Слово или выражение из текста вместе с найденными для него статьями.
#[derive(Debug)]
pub struct TermMatch {
    pub term: String,
    pub articles: Vec<Article>,
}

/// Выражение, найденное в начале последовательности слов.
struct PhraseMatch {
    words: usize,
    term_match: TermMatch,
}

/// Ищет выражения распознанного текста в словарях.
pub struct Lookup {
    dictionaries: Arc<dyn Dictionaries>,
}

impl Lookup {
    const LONGEST_PHRASE: usize = 4;

    pub fn new(dictionaries: Arc<dyn Dictionaries>) -> Self {
        Self { dictionaries }
    }

    /// Разбирает текст на самые длинные выражения, которые есть в словарях.
    pub fn terms(&self, text: &str) -> Result<Vec<TermMatch>> {
        let mut words = Self::words(text);
        let mut matches: Vec<TermMatch> = Vec::new();
        let mut start = 0;
        while start < words.len() {
            match self.longest_match(&words[start..])? {
                Some(found) => {
                    if !matches.iter().any(|known| known.term == found.term_match.term) {
                        matches.push(found.term_match);
                    }
                    start += found.words;
                }
                None if words[start].contains('-') => {
                    let parts: Vec<String> = words[start]
                        .split('-')
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned)
                        .collect();
                    words.splice(start..=start, parts);
                }
                None => start += 1,
            }
        }
        Ok(matches)
    }

    fn longest_match(&self, words: &[String]) -> Result<Option<PhraseMatch>> {
        for length in (1..=Self::LONGEST_PHRASE.min(words.len())).rev() {
            for candidate in Self::candidates(&words[..length]) {
                let articles = self.dictionaries.articles(&HeadwordKey::new(&candidate))?;
                if !articles.is_empty() {
                    let term_match = TermMatch { term: candidate, articles };
                    return Ok(Some(PhraseMatch { words: length, term_match }));
                }
            }
        }
        Ok(None)
    }

    /// Выражение как есть, затем с начальной формой первого слова (breaks out)
    /// и последнего (ice creams).
    fn candidates(phrase: &[String]) -> Vec<String> {
        let mut candidates = vec![phrase.join(" ")];
        let last = phrase.len() - 1;
        for form in english::base_forms(&phrase[0]) {
            let mut inflected = phrase.to_vec();
            inflected[0] = form;
            candidates.push(inflected.join(" "));
        }
        if last > 0 {
            for form in english::base_forms(&phrase[last]) {
                let mut inflected = phrase.to_vec();
                inflected[last] = form;
                candidates.push(inflected.join(" "));
            }
        }
        candidates
    }

    fn words(text: &str) -> Vec<String> {
        let joined = text.replace("-\n", "").replace('’', "'").to_lowercase();
        joined
            .split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '-'))
            .map(|word| word.trim_matches(|c| c == '\'' || c == '-'))
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct InMemoryDictionaries(HashMap<String, String>);

    impl InMemoryDictionaries {
        fn with(headwords: &[&str]) -> Self {
            Self(headwords.iter().map(|word| (word.to_string(), format!("{word}: перевод"))).collect())
        }
    }

    impl Dictionaries for InMemoryDictionaries {
        fn articles(&self, key: &HeadwordKey) -> Result<Vec<Article>> {
            Ok(self
                .0
                .get(key.as_str())
                .map(|body| Article {
                    dictionary: "test".into(),
                    headword: key.as_str().into(),
                    body: body.clone(),
                })
                .into_iter()
                .collect())
        }
    }

    fn terms(dictionary: &[&str], text: &str) -> Vec<String> {
        Lookup::new(Arc::new(InMemoryDictionaries::with(dictionary)))
            .terms(text)
            .unwrap()
            .into_iter()
            .map(|entry| entry.term)
            .collect()
    }

    #[test]
    fn prefers_phrase_over_single_words() {
        let dictionary = ["break", "break out", "out", "prison"];
        assert_eq!(terms(&dictionary, "He breaks out of prison."), ["break out", "prison"]);
    }

    #[test]
    fn splits_hyphenated_word_missing_from_dictionary() {
        let dictionary = ["self-employed", "co", "executive"];
        assert_eq!(terms(&dictionary, "self-employed co-executive"), ["self-employed", "co", "executive"]);
    }

    #[test]
    fn joins_word_wrapped_by_hyphen() {
        assert_eq!(terms(&["dictionary"], "dictio-\nnary"), ["dictionary"]);
    }
}
