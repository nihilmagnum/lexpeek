use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::glib;
use rust_i18n::t;

use crate::application::selection::SelectionTranslation;
use crate::application::speech::ReadingSpeech;
use crate::ui::action::Activation;
use crate::ui::background;
use crate::ui::escape;
use crate::ui::interface_language::InterfaceLanguage;
use crate::ui::keep_alive;
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
            let message = match background::run(move || reader.selected_text()).await {
                Ok(Some(text)) => {
                    Self::open(&app, &activation, speech, &t!("reading.translating")).translate(selection, text).await;
                    return;
                }
                Ok(None) => t!("reading.no_selection"),
                Err(error) => t!("reading.reading_selection_failed", error = format!("{error:#}")),
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
        escape::closes_window(window.upcast_ref());
        keep_alive::until_destroyed(&window, Rc::clone(&popup));
        activation.present(&window);
        popup
    }

    async fn translate(&self, selection: Arc<SelectionTranslation>, text: String) {
        let labels_language = InterfaceLanguage::current();
        match background::run(move || selection.read(&text, &labels_language)).await {
            Ok(reading) => self.card.show_reading(&reading),
            Err(error) => self.card.show_message(&t!("reading.translation_failed", error = format!("{error:#}"))),
        }
    }

    fn focus_changed(&self, active: bool) {
        if active {
            self.was_active.set(true);
        } else if self.was_active.replace(false) {
            self.window.close();
        }
    }
}
