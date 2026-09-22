/// Текст для перевода: строки, на которые его разбил экран, собраны обратно в абзацы.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceText(String);

impl SourceText {
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
