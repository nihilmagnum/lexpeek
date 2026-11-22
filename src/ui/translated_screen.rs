use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gio, glib};
use image::RgbaImage;
use rust_i18n::t;

use crate::application::screen_translation::ScreenTranslation;
use crate::ui::action::Activation;
use crate::ui::frozen_screen::Frame;
use crate::ui::screen_frame::ScreenFrame;
use crate::ui::translation_layer::TranslationLayer;

/// Frozen frame of the screen with all its text translated; a click or Esc closes it.
pub struct TranslatedScreen {
    frame: Rc<ScreenFrame>,
    layer: TranslationLayer,
    status: gtk::Label,
    translation: Arc<ScreenTranslation>,
    /// Number of the latest translation: the frame of another monitor makes earlier ones outdated.
    generation: Cell<u64>,
}

impl TranslatedScreen {
    pub fn show(
        app: &gtk::Application,
        screenshot: Rc<RgbaImage>,
        translation: Arc<ScreenTranslation>,
        activation: &Activation,
    ) -> Rc<ScreenFrame> {
        let frame = ScreenFrame::new(app, screenshot);
        let layer = TranslationLayer::new();
        let status = gtk::Label::builder()
            .css_classes(["lexpeek-card"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .wrap(true)
            .max_width_chars(60)
            .visible(false)
            .build();
        frame.layers().add_overlay(&layer);
        frame.layers().add_overlay(&status);

        let screen = Rc::new(Self { frame: Rc::clone(&frame), layer, status, translation, generation: Cell::new(0) });
        frame.connect_screen(glib::clone!(
            #[weak]
            screen,
            move |image| screen.translate(image)
        ));
        screen.close_on_click();
        // Other handlers hold the screen weakly; it lives as long as the frame.
        frame.connect_closed(Box::new(move || {
            let _ = &screen;
        }));

        frame.present(activation);
        frame
    }

    fn close_on_click(self: &Rc<Self>) {
        let click = gtk::GestureClick::new();
        click.connect_released(glib::clone!(
            #[weak(rename_to = screen)]
            self,
            move |_, _, _, _| screen.frame.close()
        ));
        self.frame.window().add_controller(click);
    }

    fn translate(self: &Rc<Self>, image: &RgbaImage) {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.layer.show(image, Vec::new());
        self.show_status(&t!("reading.translating"));
        let screen = Rc::clone(self);
        let translation = Arc::clone(&self.translation);
        let image = Arc::new(image.clone());
        let recognized = Arc::clone(&image);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || translation.translate(&recognized)).await;
            if screen.generation.get() != generation {
                return;
            }
            match result {
                Ok(Ok(paragraphs)) if paragraphs.is_empty() => screen.show_status(&t!("screen.nothing_to_translate")),
                Ok(Ok(paragraphs)) => {
                    screen.status.set_visible(false);
                    screen.layer.show(&image, paragraphs);
                }
                Ok(Err(error)) => screen.show_status(&t!("reading.translation_failed", error = format!("{error:#}"))),
                Err(_) => screen.show_status(&t!("reading.translation_crashed")),
            }
        });
    }

    fn show_status(&self, message: &str) {
        self.status.set_label(message);
        self.status.set_visible(true);
    }
}
