use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use ksni::TrayMethods;

use crate::application::desktop::{GlobalShortcuts, LoginAutostart, ScreenCapture, ShortcutPress, ShortcutSpec};
use crate::application::languages::LanguagesService;
use crate::application::reading::TextReading;
use crate::application::selection::SelectionTranslation;
use crate::application::speech::ReadingSpeech;
use crate::application::translators::Translators;
use crate::ui::action::{Action, Activation};
use crate::ui::overlay::Overlay;
use crate::ui::selection_popup::SelectionPopup;
use crate::ui::settings_window::SettingsWindow;
use crate::ui::shortcuts_window::ShortcutsWindow;
use crate::ui::tray::TrayIcon;

/// Use cases and desktop capabilities the application works with.
pub struct Features {
    pub reading: Arc<TextReading>,
    pub languages: Arc<LanguagesService>,
    pub translators: Arc<Translators>,
    pub selection: Arc<SelectionTranslation>,
    pub speech: Arc<ReadingSpeech>,
    pub screen: Arc<dyn ScreenCapture>,
    pub shortcuts: Arc<dyn GlobalShortcuts>,
    pub autostart: Arc<dyn LoginAutostart>,
}

/// Background application with an icon in the top bar.
pub struct App {
    application: gtk::Application,
    reading: Arc<TextReading>,
    languages: Arc<LanguagesService>,
    translators: Arc<Translators>,
    selection: Arc<SelectionTranslation>,
    speech: Arc<ReadingSpeech>,
    screen: Arc<dyn ScreenCapture>,
    shortcuts: Arc<dyn GlobalShortcuts>,
}

impl App {
    pub const ID: &str = "io.github.nihilmagnum.Lexpeek";
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
        .lexpeek-part-of-speech { opacity: 0.55; font-size: 0.85em; margin-top: 4px; }
        .lexpeek-section { font-weight: bold; margin-top: 6px; }
        .lexpeek-dim { opacity: 0.7; }
        window.lexpeek-popup { background: transparent; }
    ";

    /// The first launch only appears in the top bar, later ones open the frame; in background mode
    /// a repeated launch does nothing.
    pub fn run(features: Features, background: bool) -> glib::ExitCode {
        let application = gtk::Application::builder().application_id(Self::ID).build();
        if let Err(error) = application.register(gio::Cancellable::NONE) {
            eprintln!("lexpeek: {error}");
            return glib::ExitCode::FAILURE;
        }
        if application.is_remote() {
            return if background { glib::ExitCode::SUCCESS } else { application.run_with_args::<&str>(&[]) };
        }
        let Features { reading, languages, translators, selection, speech, screen, shortcuts, autostart } = features;
        let app = Rc::new(Self {
            application: application.clone(),
            reading,
            languages,
            translators,
            selection,
            speech,
            screen,
            shortcuts,
        });
        // The first instance stays in the background so that later launches reach it immediately.
        let _hold = application.hold();
        // GTK is already initialized: register() sends startup to the first instance itself.
        Self::load_style();
        app.download_pair_models();
        app.show_tray_icon(autostart);
        app.listen_shortcuts();
        // run() activates the first instance right away, but the frame must open only on request.
        let skip_activation = Cell::new(true);
        application.connect_activate(move |_| {
            if !skip_activation.replace(false) {
                app.perform(Action::CaptureArea, Activation::default());
            }
        });
        application.run_with_args::<&str>(&[])
    }

    fn perform(&self, action: Action, activation: Activation) {
        match action {
            Action::CaptureArea => self.capture_area(activation),
            Action::TranslateSelection => {
                let speech = Arc::clone(&self.speech);
                SelectionPopup::show(&self.application, Arc::clone(&self.selection), speech, activation)
            }
            Action::CopyTranslation => self.run_without_window(|selection| selection.copy()),
            Action::ReplaceSelection => self.run_without_window(|selection| selection.replace()),
            Action::Settings => {
                SettingsWindow::show(&self.application, Arc::clone(&self.languages), Arc::clone(&self.translators))
            }
            Action::Shortcuts => ShortcutsWindow::show(&self.application, Arc::clone(&self.shortcuts)),
            Action::Quit => self.application.quit(),
        }
    }

    fn capture_area(&self, activation: Activation) {
        if let Some(window) = self.application.active_window() {
            window.present();
            return;
        }
        let application = self.application.clone();
        let reading = Arc::clone(&self.reading);
        let speech = Arc::clone(&self.speech);
        let screen = Arc::clone(&self.screen);
        glib::spawn_future_local(async move {
            match gio::spawn_blocking(move || screen.capture()).await {
                Ok(Ok(screenshot)) => Overlay::show(&application, screenshot, reading, speech, &activation),
                Ok(Err(error)) => eprintln!("lexpeek: {error:#}"),
                Err(_) => eprintln!("lexpeek: screen capture crashed"),
            }
        });
    }

    /// For an action without a window a failure is visible only in a notification.
    fn run_without_window(&self, work: impl FnOnce(&SelectionTranslation) -> anyhow::Result<()> + Send + 'static) {
        let selection = Arc::clone(&self.selection);
        let application = self.application.clone();
        glib::spawn_future_local(async move {
            let message = match gio::spawn_blocking(move || work(&selection)).await {
                Ok(Ok(())) => return,
                Ok(Err(error)) => format!("{error:#}"),
                Err(_) => "translation crashed".to_owned(),
            };
            let notification = gio::Notification::new("Lexpeek не смог перевести выделенный текст");
            notification.set_body(Some(&message));
            application.send_notification(None, &notification);
        });
    }

    /// Models for the chosen language pair, for example on first launch.
    fn download_pair_models(&self) {
        let languages = Arc::clone(&self.languages);
        glib::spawn_future_local(async move {
            if let Ok(Err(error)) = gio::spawn_blocking(move || languages.download_pair_models()).await {
                eprintln!("lexpeek: {error:#}");
            }
        });
    }

    /// The port listens for presses while the application runs, so it takes its own thread, and presses
    /// are passed to the GTK main thread.
    fn listen_shortcuts(self: &Rc<Self>) {
        let (presses, received) = async_channel::unbounded::<ShortcutPress>();
        let shortcuts = Arc::clone(&self.shortcuts);
        std::thread::spawn(move || {
            let specs: Vec<ShortcutSpec> = Action::WITH_SHORTCUT.into_iter().filter_map(Action::shortcut).collect();
            let on_press = Box::new(move |press| {
                // The channel is closed only when the application is already quitting.
                let _ = presses.send_blocking(press);
            });
            if let Err(error) = shortcuts.listen(&specs, on_press) {
                eprintln!("lexpeek: global shortcuts are unavailable: {error:#}");
            }
        });
        let app = Rc::clone(self);
        glib::spawn_future_local(async move {
            while let Ok(press) = received.recv().await {
                if let Some(action) = Action::by_shortcut_id(&press.id) {
                    app.perform(action, Activation::with_token(press.activation_token));
                }
            }
        });
    }

    fn show_tray_icon(self: &Rc<Self>, autostart: Arc<dyn LoginAutostart>) {
        let (actions, received) = async_channel::unbounded();
        let tray = TrayIcon::new(Self::ID, autostart, actions);
        let app = Rc::clone(self);
        glib::spawn_future_local(async move {
            let _tray = match tray.spawn().await {
                Ok(tray) => tray,
                Err(error) => return eprintln!("lexpeek: cannot show tray icon: {error}"),
            };
            while let Ok(action) = received.recv().await {
                app.perform(action, Activation::default());
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
