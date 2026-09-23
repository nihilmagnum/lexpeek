/// Словарная статья слова или короткого выражения.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DictionaryArticle {
    /// Произношение оригинала латиницей.
    pub transcription: Option<String>,
    pub meanings: Vec<Meaning>,
    pub definitions: Vec<Definition>,
    pub examples: Vec<String>,
}

impl DictionaryArticle {
    pub fn is_empty(&self) -> bool {
        self.meanings.is_empty() && self.definitions.is_empty() && self.examples.is_empty()
    }
}

/// Варианты перевода для одной части речи, от самого употребительного.
#[derive(Clone, Debug, PartialEq)]
pub struct Meaning {
    pub part_of_speech: String,
    pub translations: Vec<MeaningTranslation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningTranslation {
    pub word: String,
    /// Близкие по смыслу варианты перевода.
    pub synonyms: Vec<String>,
    /// Слова оригинала, которые тоже переводятся этим вариантом.
    pub back_translations: Vec<String>,
}

/// Толкование на языке оригинала.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub part_of_speech: String,
    pub gloss: String,
    pub example: Option<String>,
    pub synonyms: Vec<String>,
}
