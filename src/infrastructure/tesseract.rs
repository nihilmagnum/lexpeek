use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use image::imageops::{self, FilterType};
use image::{GrayImage, RgbaImage};
use tesseract::{PageSegMode, Tesseract};

use crate::application::languages::OcrModelStore;
use crate::application::reading::TextRecognizer;
use crate::application::screen_translation::ParagraphRecognizer;
use crate::domain::language::Language;
use crate::domain::text::{ScreenArea, ScreenParagraph};
use crate::infrastructure::atomic_file;

/// Word of the TSV output of Tesseract with its place in the layout of the page.
struct RecognizedWord {
    block: u32,
    paragraph: u32,
    line: u32,
    area: ScreenArea,
    confidence: f64,
    text: String,
}

impl RecognizedWord {
    const LEVEL: &str = "5";

    /// Columns: level, page, block, paragraph, line, word, left, top, width, height, confidence, text.
    /// Rows of other levels describe the layout and carry no text.
    fn parse(row: &str) -> Option<Self> {
        let columns: Vec<&str> = row.split('\t').collect();
        let [level, _, block, paragraph, line, _, left, top, width, height, confidence, text] = columns[..] else {
            return None;
        };
        if level != Self::LEVEL || text.trim().is_empty() {
            return None;
        }
        let area = ScreenArea {
            x: left.parse().ok()?,
            y: top.parse().ok()?,
            width: width.parse().ok()?,
            height: height.parse().ok()?,
        };
        Some(Self {
            block: block.parse().ok()?,
            paragraph: paragraph.parse().ok()?,
            line: line.parse().ok()?,
            area,
            // Tesseract writes the confidence in the locale of the process, which GTK takes from the environment.
            confidence: confidence.replace(',', ".").parse().ok()?,
            text: text.trim().to_owned(),
        })
    }

    fn same_paragraph(&self, other: &Self) -> bool {
        self.block == other.block && self.paragraph == other.paragraph
    }

    fn same_line(&self, other: &Self) -> bool {
        self.same_paragraph(other) && self.line == other.line
    }
}

/// Text recognition with the Tesseract engine.
pub struct TesseractRecognizer {
    models: TesseractModels,
}

impl TesseractRecognizer {
    /// Screen fonts are smaller than Tesseract is designed for, so the fragment is scaled up
    /// to roughly this width.
    const TARGET_WIDTH: u32 = 1200;
    /// The whole screen is scaled up less: recognition time grows with the size of the image.
    const SCREEN_TARGET_WIDTH: u32 = 3840;
    const MAX_UPSCALE: u32 = 4;
    /// Below it words are mostly icons and pictures taken for text.
    const MIN_CONFIDENCE: f64 = 50.0;
    /// Tesseract may put list items, such as menu entries, into one paragraph. They are told apart from lines of
    /// a paragraph by a gap wider than this share of the line height.
    const PARAGRAPH_GAP: f64 = 0.75;
    /// Characters reaching below the baseline. A line without them spans only the part of the font above it.
    const DESCENDERS: &str = "gjpqyQдруфцщДЦЩ(),;[]{}";
    /// Share of the font size above the baseline.
    const ASCENT: f64 = 0.75;

    pub fn new(models: TesseractModels) -> Self {
        Self { models }
    }

    /// Tesseract after recognizing the image.
    fn recognized(&self, image: &GrayImage, languages: &[Language], mode: PageSegMode) -> Result<Tesseract> {
        for language in languages {
            ensure!(self.models.is_installed(language), "OCR model for {} is not downloaded", language.code());
        }
        let names: Vec<&str> = languages.iter().map(TesseractModels::model_name).collect();
        let directory = self.models.directory().to_string_lossy();
        let (width, height) = image.dimensions();
        let mut engine = Tesseract::new(Some(&directory), Some(&names.join("+")))
            .context("cannot initialize Tesseract")?
            .set_frame(image.as_raw(), width as i32, height as i32, 1, width as i32)?
            .set_source_resolution(300);
        engine.set_page_seg_mode(mode);
        Ok(engine.recognize()?)
    }

    fn scale(width: u32, target_width: u32) -> u32 {
        (target_width / width.max(1)).clamp(1, Self::MAX_UPSCALE)
    }

    /// Grayscale image with dark text on a light background, enlarged for recognition.
    fn prepare(image: &RgbaImage, scale: u32) -> GrayImage {
        let mut gray = imageops::grayscale(image);
        let pixels = u64::from(gray.width()) * u64::from(gray.height());
        let brightness = gray.pixels().map(|pixel| u64::from(pixel.0[0])).sum::<u64>() / pixels.max(1);
        if brightness < 128 {
            imageops::invert(&mut gray);
        }
        if scale == 1 {
            return gray;
        }
        imageops::resize(&gray, gray.width() * scale, gray.height() * scale, FilterType::CatmullRom)
    }

    /// Paragraphs of the TSV output of an image enlarged `scale` times.
    fn paragraphs(tsv: &str, scale: u32) -> Vec<ScreenParagraph> {
        let words: Vec<RecognizedWord> = tsv.lines().filter_map(RecognizedWord::parse).collect();
        let lines: Vec<&[RecognizedWord]> = words.chunk_by(RecognizedWord::same_line).collect();
        lines
            .chunk_by(|upper, lower| Self::continues_paragraph(upper, lower))
            .filter_map(|lines| Self::paragraph(lines, scale))
            .collect()
    }

    fn continues_paragraph(upper: &[RecognizedWord], lower: &[RecognizedWord]) -> bool {
        let (upper_area, lower_area) = (Self::area(upper), Self::area(lower));
        let gap = lower_area.y.saturating_sub(upper_area.bottom());
        upper[0].same_paragraph(&lower[0]) && f64::from(gap) <= Self::PARAGRAPH_GAP * f64::from(upper_area.height)
    }

    /// Paragraphs of words recognized with low confidence or without letters are dropped.
    fn paragraph(lines: &[&[RecognizedWord]], scale: u32) -> Option<ScreenParagraph> {
        let words: Vec<&RecognizedWord> = lines.iter().flat_map(|line| line.iter()).collect();
        let confidence = words.iter().map(|word| word.confidence).sum::<f64>() / words.len() as f64;
        let text = lines
            .iter()
            .map(|line| line.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        if confidence < Self::MIN_CONFIDENCE || !text.chars().any(char::is_alphabetic) {
            return None;
        }
        let area = lines.iter().map(|line| Self::area(line)).reduce(ScreenArea::union)?;
        let font_size = lines.iter().map(|line| Self::font_size(line)).sum::<f64>() / lines.len() as f64;
        let [x, y, width, height] = [area.x, area.y, area.width, area.height].map(|value| value / scale);
        let font_size = (font_size / f64::from(scale)).round() as u32;
        Some(ScreenParagraph { text, area: ScreenArea { x, y, width, height }, font_size })
    }

    fn font_size(line: &[RecognizedWord]) -> f64 {
        let height = f64::from(Self::area(line).height);
        let descends = line.iter().any(|word| word.text.contains(|letter| Self::DESCENDERS.contains(letter)));
        if descends { height } else { height / Self::ASCENT }
    }

    fn area(words: &[RecognizedWord]) -> ScreenArea {
        words.iter().map(|word| word.area).reduce(ScreenArea::union).expect("a line has words")
    }
}

impl TextRecognizer for TesseractRecognizer {
    fn recognize(&self, fragment: &RgbaImage, languages: &[Language]) -> Result<String> {
        let prepared = Self::prepare(fragment, Self::scale(fragment.width(), Self::TARGET_WIDTH));
        let mut engine = self.recognized(&prepared, languages, PageSegMode::PsmSingleBlock)?;
        Ok(engine.get_text()?.trim().to_owned())
    }
}

impl ParagraphRecognizer for TesseractRecognizer {
    fn recognize_paragraphs(&self, screen: &RgbaImage, languages: &[Language]) -> Result<Vec<ScreenParagraph>> {
        let scale = Self::scale(screen.width(), Self::SCREEN_TARGET_WIDTH);
        let mut engine = self.recognized(&Self::prepare(screen, scale), languages, PageSegMode::PsmAuto)?;
        Ok(Self::paragraphs(&engine.get_tsv_text(0)?, scale))
    }
}

/// Tesseract recognition models in the lexpeek directory.
#[derive(Clone)]
pub struct TesseractModels {
    directory: PathBuf,
}

impl TesseractModels {
    const DOWNLOAD_URL: &str = "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main";

    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Tesseract model name for a language; the model of a user-added language is named by its code.
    pub fn model_name(language: &Language) -> &str {
        match language.code().as_str() {
            "ru" => "rus",
            "en" => "eng",
            "de" => "deu",
            "fr" => "fra",
            "es" => "spa",
            "it" => "ita",
            "pt" => "por",
            "uk" => "ukr",
            "pl" => "pol",
            "tr" => "tur",
            "ja" => "jpn",
            "zh" => "chi_sim",
            "ko" => "kor",
            code => code,
        }
    }

    fn install(&self, language: &Language, model: &[u8]) -> Result<()> {
        atomic_file::replace_file(&self.path(language), |partial| fs::write(partial, model))
    }

    fn path(&self, language: &Language) -> PathBuf {
        self.directory.join(format!("{}.traineddata", Self::model_name(language)))
    }
}

impl OcrModelStore for TesseractModels {
    fn is_installed(&self, language: &Language) -> bool {
        self.path(language).exists()
    }

    fn download(&self, language: &Language) -> Result<()> {
        let url = format!("{}/{}.traineddata", Self::DOWNLOAD_URL, Self::model_name(language));
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(300))).build();
        let agent: ureq::Agent = config.into();
        let model = agent
            .get(&url)
            .call()
            .with_context(|| format!("cannot download {url}"))?
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_vec()
            .with_context(|| format!("cannot download {url}"))?;

        self.install(language, &model)
    }

    fn install_from_file(&self, language: &Language, file: &Path) -> Result<()> {
        let model = fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
        self.install(language, &model)
    }

    fn remove(&self, language: &Language) -> Result<()> {
        let path = self.path(language);
        fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::language::{CustomLanguage, LanguageCode};

    #[test]
    fn groups_lines_into_paragraphs_and_separates_list_items() {
        let tsv = [
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext",
            "4\t1\t1\t1\t1\t0\t50\t165\t70\t21\t-1\t",
            "5\t1\t1\t1\t1\t1\t50\t165\t70\t21\t93.3\tInbox",
            "5\t1\t1\t1\t2\t1\t50\t237\t99\t21\t96.7\tStarred",
            "5\t1\t2\t1\t1\t1\t720\t244\t50\t22\t96.5\tThe",
            "5\t1\t2\t1\t1\t2\t782\t244\t73\t28\t96,7\tquick",
            "5\t1\t2\t1\t2\t1\t722\t284\t62\t22\t96.1\tfox.",
            "5\t1\t3\t1\t1\t1\t0\t0\t609\t2160\t95.0\t ",
            "5\t1\t4\t1\t1\t1\t900\t900\t30\t30\t21.0\tW",
        ]
        .join("\n");

        let paragraphs = TesseractRecognizer::paragraphs(&tsv, 2);
        let texts: Vec<&str> = paragraphs.iter().map(|paragraph| paragraph.text.as_str()).collect();
        assert_eq!(texts, ["Inbox", "Starred", "The quick\nfox."]);
        assert_eq!(paragraphs[2].area, ScreenArea { x: 360, y: 122, width: 67, height: 31 });
        assert_eq!(paragraphs[2].font_size, 14);
    }

    #[test]
    fn installs_custom_language_model_from_file_and_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("any-name.traineddata");
        fs::write(&file, b"model").unwrap();
        let models = TesseractModels::new(directory.path().join("tessdata"));
        let georgian = Language::Custom(CustomLanguage::new(LanguageCode::parse("ka").unwrap(), "Georgian").unwrap());

        models.install_from_file(&georgian, &file).unwrap();
        assert!(models.is_installed(&georgian));
        assert_eq!(fs::read(directory.path().join("tessdata/ka.traineddata")).unwrap(), b"model");

        models.remove(&georgian).unwrap();
        assert!(!models.is_installed(&georgian));
    }
}
