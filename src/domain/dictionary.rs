/// Нормализованный заголовок словарной статьи, по которому ищут.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadwordKey(String);

impl HeadwordKey {
    pub fn new(headword: &str) -> Self {
        let normalized = headword.replace('’', "'").to_lowercase();
        Self(normalized.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Article {
    pub dictionary: String,
    pub headword: String,
    pub body: String,
}

/// Словарь целиком, как его импортируют.
pub struct Dictionary {
    pub name: String,
    pub entries: Vec<DictionaryEntry>,
}

pub struct DictionaryEntry {
    pub headword: String,
    /// Синонимы и словоформы, которые ведут на эту статью.
    pub aliases: Vec<String>,
    pub body: String,
}
