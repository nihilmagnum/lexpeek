use crate::domain::language::LanguageCode;

/// Text in a known language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phrase {
    pub text: String,
    pub language: LanguageCode,
}

/// Text to translate.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceText(String);

impl SourceText {
    /// Lines the text is broken into on the screen are joined back into paragraphs.
    pub fn from_recognized(recognized: &str) -> Self {
        let unwrapped = recognized.replace("-\n", "");
        let paragraphs: Vec<String> = unwrapped
            .split("\n\n")
            .map(|paragraph| paragraph.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|paragraph| !paragraph.is_empty())
            .collect();
        Self(paragraphs.join("\n"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SourceText;

    #[test]
    fn rejoins_lines_into_paragraphs() {
        let recognized = "The door is lo-\ncked, find\nthe key.\n\nPress E";
        assert_eq!(SourceText::from_recognized(recognized).as_str(), "The door is locked, find the key.\nPress E");
    }
}
