mod application;
mod domain;
mod infrastructure;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use directories::{BaseDirs, ProjectDirs};

use crate::application::languages::LanguagesService;
use crate::application::reading::TextReading;
use crate::application::selection::SelectionTranslation;
use crate::application::speech::ReadingSpeech;
use crate::application::translators::{Translator, Translators};
use crate::infrastructure::autostart::DesktopEntryAutostart;
use crate::infrastructure::google::GoogleTranslator;
use crate::infrastructure::google_speech::GoogleSpeaker;
use crate::infrastructure::portal::PortalAccess;
use crate::infrastructure::remote_keyboard::RemoteKeyboard;
use crate::infrastructure::screenshot::PortalScreenCapture;
use crate::infrastructure::settings_file::SettingsFile;
use crate::infrastructure::shortcut::PortalShortcuts;
use crate::infrastructure::tesseract::{TesseractModels, TesseractRecognizer};
use crate::infrastructure::wl_clipboard::WlClipboard;
use crate::infrastructure::yandex::YandexTranslator;
use crate::ui::app::{App, Features};

/// Translate text on the screen: select an area or text, and lexpeek shows the translation.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Do not open the frame if lexpeek is already running (for autostart)
    #[arg(long)]
    background: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = ProjectDirs::from("", "", "lexpeek")
        .context("home directory is unknown")
        .and_then(|directories| Ok(App::run(features(&directories)?, cli.background)));
    match result {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("lexpeek: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn features(directories: &ProjectDirs) -> Result<Features> {
    if let Err(error) = PortalAccess::register(App::ID) {
        eprintln!("lexpeek: cannot register in portals: {error:#}");
    }
    if let Err(error) = PortalAccess::allow_screenshots(App::ID) {
        eprintln!("lexpeek: cannot allow screenshots: {error:#}");
    }
    let settings = Arc::new(SettingsFile::new(directories.config_dir().join("settings.json")));
    let models = TesseractModels::new(directories.data_dir().join("tessdata"));
    let languages = Arc::new(LanguagesService::new(settings.clone(), Arc::new(models.clone())));
    let available: Vec<Arc<dyn Translator>> =
        vec![Arc::new(GoogleTranslator::new()), Arc::new(YandexTranslator::new())];
    let translators = Arc::new(Translators::new(available, settings.clone()));
    let reading =
        Arc::new(TextReading::new(settings, Arc::new(TesseractRecognizer::new(models)), Arc::clone(&translators)));
    let clipboard = Arc::new(WlClipboard);
    let selection = SelectionTranslation::new(
        clipboard.clone(),
        Arc::clone(&reading),
        clipboard,
        Arc::new(RemoteKeyboard::new(directories.data_dir().join("remote-desktop-token"))),
    );
    Ok(Features {
        reading,
        languages,
        translators,
        selection: Arc::new(selection),
        speech: Arc::new(ReadingSpeech::new(Arc::new(GoogleSpeaker::new()))),
        screen: Arc::new(PortalScreenCapture),
        shortcuts: Arc::new(PortalShortcuts::new(App::ID)),
        autostart: Arc::new(autostart()?),
    })
}

fn autostart() -> Result<DesktopEntryAutostart> {
    let directories = BaseDirs::new().context("home directory is unknown")?;
    let binary = std::env::current_exe().context("cannot locate lexpeek binary")?;
    Ok(DesktopEntryAutostart::new(directories.config_dir(), App::ID, binary))
}
