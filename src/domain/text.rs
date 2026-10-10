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

/// Rectangle on the screen in screenshot pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenArea {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScreenArea {
    pub fn bottom(self) -> u32 {
        self.y + self.height
    }

    /// Smallest area covering both.
    pub fn union(self, other: Self) -> Self {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        let right = (self.x + self.width).max(other.x + other.width);
        Self { x, y, width: right - x, height: self.bottom().max(other.bottom()) - y }
    }
}

/// Paragraph of text found on the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenParagraph {
    /// Recognized text broken into lines as on the screen.
    pub text: String,
    pub area: ScreenArea,
    /// Approximate size of the font in screenshot pixels.
    pub font_size: u32,
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
