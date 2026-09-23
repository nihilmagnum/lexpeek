use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::application::selection::SelectionTranslation;
use crate::ui::action::Activation;
use crate::ui::reading_card::ReadingCard;

/// Небольшое окно с переводом выделенного текста.
pub struct SelectionPopup {
    window: gtk::ApplicationWindow,
    card: ReadingCard,
    /// Окно закрывается, когда фокус уходит, но только после того, как оно его получило.
    was_active: Cell<bool>,
}

impl SelectionPopup {
    pub fn show(app: &gtk::Application, selection: Arc<SelectionTranslation>, activation: &Activation) {
        let card = ReadingCard::new();
        card.widget().set_margin_top(12);
        card.widget().set_margin_bottom(12);
        card.widget().set_margin_start(12);
        card.widget().set_margin_end(12);
        card.show_message("Перевожу…");
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
        // Обработчики держат окно слабой ссылкой, живёт оно столько же, сколько окно GTK.
        window.connect_destroy(glib::clone!(
            #[strong]
            popup,
            move |_| {
                let _ = &popup;
            }
        ));
        activation.present(&window);

        glib::spawn_future_local(async move {
            let reading = gio::spawn_blocking(move || selection.read()).await;
            match reading {
                Ok(Ok(Some(reading))) => popup.card.show_reading(&reading),
                Ok(Ok(None)) => popup.card.show_message("Нет выделенного текста"),
                Ok(Err(error)) => popup.card.show_message(&format!("Не удалось перевести: {error:#}")),
                Err(_) => popup.card.show_message("Перевод аварийно завершился"),
            }
        });
    }

    fn connect_escape(self: &Rc<Self>) {
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
