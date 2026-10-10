use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::application::selection::SelectionTranslation;
use crate::application::speech::ReadingSpeech;
use crate::ui::action::Activation;
use crate::ui::reading_card::ReadingCard;

/// Small window with the translation of the selected text.
pub struct SelectionPopup {
    window: gtk::ApplicationWindow,
    card: ReadingCard,
    /// The window closes when it loses focus, but only after it has received focus.
    was_active: Cell<bool>,
}

impl SelectionPopup {
    /// The selection is read before the window appears: reading takes focus for a moment,
    /// and the window closes when it loses focus.
    pub fn show(
        app: &gtk::Application,
        selection: Arc<SelectionTranslation>,
        speech: Arc<ReadingSpeech>,
        activation: Activation,
    ) {
        let app = app.clone();
        glib::spawn_future_local(async move {
            let reader = Arc::clone(&selection);
            let message = match gio::spawn_blocking(move || reader.selected_text()).await {
                Ok(Ok(Some(text))) => {
                    Self::open(&app, &activation, speech, "Перевожу…").translate(selection, text).await;
                    return;
                }
                Ok(Ok(None)) => "Нет выделенного текста".to_owned(),
                Ok(Err(error)) => format!("Не удалось прочитать выделение: {error:#}"),
                Err(_) => "Чтение выделения аварийно завершилось".to_owned(),
            };
            Self::open(&app, &activation, speech, &message);
        });
    }

    fn open(
        app: &gtk::Application,
        activation: &Activation,
        speech: Arc<ReadingSpeech>,
        message: &str,
    ) -> Rc<Self> {
        let card = ReadingCard::new(speech);
        card.widget().set_margin_top(12);
        card.widget().set_margin_bottom(12);
        card.widget().set_margin_start(12);
        card.widget().set_margin_end(12);
        card.show_message(message);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Lexpeek")
            .decorated(false)
            .resizable(false)
            .css_classes(["lexpeek-popup"])
            .child(card.widget())
            .build();

        let popup = Rc::new(Self { window: window.clone(), card, was_active: Cell::new(false) });
        window.connect_is_active_notify(glib::clone!(
            #[weak]
            popup,
            move |window| popup.focus_changed(window.is_active())
        ));
        popup.connect_escape();
        // Handlers hold the window weakly; it lives as long as the GTK window.
        window.connect_destroy(glib::clone!(
            #[strong]
            popup,
            move |_| {
                let _ = &popup;
            }
        ));
        activation.present(&window);
        popup
    }

    async fn translate(&self, selection: Arc<SelectionTranslation>, text: String) {
        match gio::spawn_blocking(move || selection.read(&text)).await {
            Ok(Ok(reading)) => self.card.show_reading(&reading),
            Ok(Err(error)) => self.card.show_message(&format!("Не удалось перевести: {error:#}")),
            Err(_) => self.card.show_message("Перевод аварийно завершился"),
        }
    }

    fn connect_escape(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = window)]
            self.window,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                window.close();
                glib::Propagation::Stop
            }
        ));
        self.window.add_controller(keys);
    }

    fn focus_changed(&self, active: bool) {
        if active {
            self.was_active.set(true);
        } else if self.was_active.replace(false) {
            self.window.close();
        }
    }
}
