/// Dictionary article for a word or a short phrase.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DictionaryArticle {
    /// Pronunciation of the original in Latin script.
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

/// Translation variants for one part of speech, most common first.
#[derive(Clone, Debug, PartialEq)]
pub struct Meaning {
    pub part_of_speech: String,
    pub translations: Vec<MeaningTranslation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningTranslation {
    pub word: String,
    /// Variants close in meaning.
    pub synonyms: Vec<String>,
    /// Words of the original that are also translated by this variant.
    pub back_translations: Vec<String>,
}

/// Definition in the language of the original.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub part_of_speech: String,
    pub gloss: String,
    pub example: Option<String>,
    pub synonyms: Vec<String>,
}
