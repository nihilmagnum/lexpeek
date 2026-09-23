use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use image::imageops::{self, FilterType};
use image::{GrayImage, RgbaImage};
use tesseract::{PageSegMode, Tesseract};

use crate::application::languages::OcrModelStore;
use crate::application::reading::TextRecognizer;
use crate::domain::language::Language;

/// Распознавание текста движком Tesseract.
pub struct TesseractRecognizer {
    models: TesseractModels,
}

impl TesseractRecognizer {
    /// Экранный шрифт мельче, чем рассчитан Tesseract, поэтому фрагмент растягивается
    /// примерно до этой ширины.
    const TARGET_WIDTH: u32 = 1200;
    const MAX_UPSCALE: u32 = 4;

    pub fn new(models: TesseractModels) -> Self {
        Self { models }
    }

    /// Серое изображение с тёмным текстом на светлом фоне, увеличенное для распознавания.
    fn prepare(fragment: &RgbaImage) -> GrayImage {
        let mut gray = imageops::grayscale(fragment);
        let pixels = u64::from(gray.width()) * u64::from(gray.height());
        let brightness = gray.pixels().map(|pixel| u64::from(pixel.0[0])).sum::<u64>() / pixels.max(1);
        if brightness < 128 {
            imageops::invert(&mut gray);
        }
        let scale = (Self::TARGET_WIDTH / gray.width().max(1)).clamp(1, Self::MAX_UPSCALE);
        if scale == 1 {
            return gray;
        }
        imageops::resize(&gray, gray.width() * scale, gray.height() * scale, FilterType::CatmullRom)
    }
}

impl TextRecognizer for TesseractRecognizer {
    fn recognize(&self, fragment: &RgbaImage, languages: &[Language]) -> Result<String> {
        for language in languages {
            ensure!(self.models.is_installed(language), "OCR model for {} is not downloaded", language.code());
        }
        let names: Vec<&str> = languages.iter().map(TesseractModels::model_name).collect();
        let directory = self.models.directory().to_string_lossy();
        let prepared = Self::prepare(fragment);
        let (width, height) = prepared.dimensions();
        let mut engine = Tesseract::new(Some(&directory), Some(&names.join("+")))
            .context("cannot initialize Tesseract")?
            .set_frame(prepared.as_raw(), width as i32, height as i32, 1, width as i32)?
            .set_source_resolution(300);
        engine.set_page_seg_mode(PageSegMode::PsmSingleBlock);
        Ok(engine.recognize()?.get_text()?.trim().to_owned())
    }
}

/// Модели распознавания Tesseract в каталоге lexpeek.
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

    /// Имя модели Tesseract для языка; модель добавленного пользователем языка названа его кодом.
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
        fs::create_dir_all(&self.directory).with_context(|| format!("cannot create {}", self.directory.display()))?;
        let path = self.path(language);
        // Недописанный файл не должен выглядеть установленной моделью.
        let partial = path.with_extension("part");
        fs::write(&partial, model).with_context(|| format!("cannot write {}", partial.display()))?;
        fs::rename(&partial, &path).with_context(|| format!("cannot write {}", path.display()))
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
    use crate::domain::language::LanguageCode;

    #[test]
    fn installs_custom_language_model_from_file_and_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("any-name.traineddata");
        fs::write(&file, b"model").unwrap();
        let models = TesseractModels::new(directory.path().join("tessdata"));
        let georgian = Language::custom(LanguageCode::parse("ka").unwrap(), "Грузинский").unwrap();

        models.install_from_file(&georgian, &file).unwrap();
        assert!(models.is_installed(&georgian));
        assert_eq!(fs::read(directory.path().join("tessdata/ka.traineddata")).unwrap(), b"model");

        models.remove(&georgian).unwrap();
        assert!(!models.is_installed(&georgian));
    }
}
