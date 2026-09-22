mod application;
mod domain;
mod infrastructure;
mod ui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use directories::{BaseDirs, ProjectDirs};

use crate::application::dictionaries::DictionaryImport;
use crate::application::languages::LanguagesService;
use crate::application::reading::ScreenReading;
use crate::domain::lookup::Lookup;
use crate::infrastructure::autostart::Autostart;
use crate::infrastructure::google::GoogleTranslator;
use crate::infrastructure::settings_file::SettingsFile;
use crate::infrastructure::sqlite_library::SqliteLibrary;
use crate::infrastructure::stardict::StarDictReader;
use crate::infrastructure::tesseract::{TesseractModels, TesseractRecognizer};
use crate::ui::app::{App, Services};

/// Перевод текста с экрана: выделите область, и lexpeek покажет перевод и статьи из словарей.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Не открывать кадр, если lexpeek уже запущен (для автозапуска)
    #[arg(long)]
    background: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Импортировать словарь StarDict по пути к .ifo
    Import { ifo: PathBuf },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = ProjectDirs::from("", "", "lexpeek").context("home directory is unknown").and_then(|directories| {
        let library = Arc::new(SqliteLibrary::open(&directories.data_dir().join("library.sqlite"))?);
        match cli.command {
            Some(Command::Import { ifo }) => import(library, &ifo),
            None => Ok(App::run(services(&directories, library)?, cli.background)),
        }
    });
    match result {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("lexpeek: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn services(directories: &ProjectDirs, library: Arc<SqliteLibrary>) -> Result<Services> {
    let settings = Arc::new(SettingsFile::new(directories.config_dir().join("settings.json")));
    let models = TesseractModels::new(directories.data_dir().join("tessdata"));
    let languages = Arc::new(LanguagesService::new(settings.clone(), Arc::new(models.clone())));
    let reading = ScreenReading::new(
        settings,
        Arc::new(TesseractRecognizer::new(models)),
        Arc::new(GoogleTranslator::new()),
        Lookup::new(library),
    );
    Ok(Services { reading: Arc::new(reading), languages, autostart: autostart()? })
}

fn import(library: Arc<SqliteLibrary>, ifo: &Path) -> Result<gtk::glib::ExitCode> {
    let imported = DictionaryImport::new(Arc::new(StarDictReader), library).import(ifo)?;
    println!("Imported {} articles from «{}»", imported.articles, imported.name);
    Ok(gtk::glib::ExitCode::SUCCESS)
}

fn autostart() -> Result<Autostart> {
    let directories = BaseDirs::new().context("home directory is unknown")?;
    let binary = std::env::current_exe().context("cannot locate lexpeek binary")?;
    Ok(Autostart::new(directories.config_dir(), App::ID, binary))
}
