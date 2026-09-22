use std::cell::Cell;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use ksni::TrayMethods;

use crate::application::languages::LanguagesService;
use crate::application::reading::ScreenReading;
use crate::infrastructure::autostart::Autostart;
use crate::infrastructure::screenshot::ScreenCapture;
use crate::infrastructure::shortcut::CaptureShortcut;
use crate::ui::languages_window::LanguagesWindow;
use crate::ui::overlay::Overlay;
use crate::ui::shortcut_editor::ShortcutEditor;
use crate::ui::tray::{TrayCommand, TrayIcon};

/// Всё, из чего собирается приложение.
pub struct Services {
    pub reading: Arc<ScreenReading>,
    pub languages: Arc<LanguagesService>,
    pub autostart: Autostart,
}

/// Фоновое приложение со значком на панели: каждый запуск `lexpeek` снимает экран и открывает кадр для выделения.
pub struct App;

impl App {
    pub const ID: &str = "io.github.lexpeek.Lexpeek";
    const STYLE: &str = "
        .lexpeek-card {
            background-color: rgb(36, 36, 40);
            color: rgb(242, 242, 242);
            border: 1px solid rgba(255, 255, 255, 0.12);
            border-radius: 12px;
            padding: 12px 14px;
            box-shadow: 0 6px 20px rgba(0, 0, 0, 0.45);
        }
        .lexpeek-recognized { opacity: 0.65; font-style: italic; }
        .lexpeek-translation { font-size: 1.1em; }
        .lexpeek-term { font-weight: bold; font-size: 1.15em; }
        .lexpeek-source { opacity: 0.55; font-size: 0.85em; margin-bottom: 4px; }
    ";

    /// Первый запуск только появляется на панели, следующие открывают кадр; в фоновом режиме
    /// повторный запуск ничего не делает.
    pub fn run(services: Services, background: bool) -> glib::ExitCode {
        let app = gtk::Application::builder().application_id(Self::ID).build();
        if let Err(error) = app.register(gio::Cancellable::NONE) {
            eprintln!("lexpeek: {error}");
            return glib::ExitCode::FAILURE;
        }
        if app.is_remote() {
            return if background { glib::ExitCode::SUCCESS } else { app.run_with_args::<&str>(&[]) };
        }
        // Портал запоминает приложение при первом обращении, поэтому регистрация идёт раньше снимков и горячих клавиш.
        let registration = async { ashpd::register_host_app(Self::ID.try_into()?).await };
        if let Err(error) = glib::MainContext::default().block_on(registration) {
            eprintln!("lexpeek: cannot register in portals: {error}");
        }

        let Services { reading, languages, autostart } = services;
        // Первый экземпляр остаётся в фоне, чтобы следующие запуски сразу доходили до него.
        let _hold = app.hold();
        // GTK уже инициализирован: register() сам отправляет startup первому экземпляру.
        Self::load_style();
        Self::download_pair_models(Arc::clone(&languages));
        Self::show_tray_icon(&app, autostart, languages);
        Self::listen_shortcut(&app);
        // run() активирует первый экземпляр сразу, а кадр должен открываться только по запросу.
        let skip_activation = Cell::new(true);
        app.connect_activate(move |app| {
            if skip_activation.replace(false) {
                return;
            }
            Self::lookup_screen(app, Arc::clone(&reading));
        });
        app.run_with_args::<&str>(&[])
    }

    fn lookup_screen(app: &gtk::Application, reading: Arc<ScreenReading>) {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let app = app.clone();
        glib::spawn_future_local(async move {
            match ScreenCapture::take().await {
                Ok(screenshot) => Overlay::show(&app, screenshot, reading),
                Err(error) => eprintln!("lexpeek: {error:#}"),
            }
        });
    }

    /// Модели для выбранной пары языков, например при первом запуске.
    fn download_pair_models(languages: Arc<LanguagesService>) {
        glib::spawn_future_local(async move {
            if let Ok(Err(error)) = gio::spawn_blocking(move || languages.download_pair_models()).await {
                eprintln!("lexpeek: {error:#}");
            }
        });
    }

    fn listen_shortcut(app: &gtk::Application) {
        let app = app.clone();
        glib::spawn_future_local(async move {
            if let Err(error) = CaptureShortcut::listen(move || app.activate()).await {
                eprintln!("lexpeek: global shortcut is unavailable: {error}");
            }
        });
    }

    fn show_tray_icon(app: &gtk::Application, autostart: Autostart, languages: Arc<LanguagesService>) {
        let (commands, received) = async_channel::unbounded();
        let app = app.clone();
        glib::spawn_future_local(async move {
            let _tray = match TrayIcon::new(Self::ID, autostart, commands).spawn().await {
                Ok(tray) => tray,
                Err(error) => return eprintln!("lexpeek: cannot show tray icon: {error}"),
            };
            while let Ok(command) = received.recv().await {
                match command {
                    TrayCommand::Capture => app.activate(),
                    TrayCommand::ChangeShortcut => ShortcutEditor::show(&app, Self::ID),
                    TrayCommand::Languages => LanguagesWindow::show(&app, Arc::clone(&languages)),
                    TrayCommand::Quit => app.quit(),
                }
            }
        });
    }

    fn load_style() {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(Self::STYLE);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
    }
}
