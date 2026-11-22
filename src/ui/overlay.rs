use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gio, glib};
use image::RgbaImage;
use rust_i18n::t;

use crate::application::reading::TextReading;
use crate::application::speech::ReadingSpeech;
use crate::ui::action::Activation;
use crate::ui::frozen_screen::Frame;
use crate::ui::interface_language::InterfaceLanguage;
use crate::ui::reading_card::{Anchor, ReadingCard};
use crate::ui::screen_frame::ScreenFrame;

/// Frozen frame of the screen on which text is selected for translation.
pub struct Overlay {
    frame: Rc<ScreenFrame>,
    canvas: gtk::DrawingArea,
    card: ReadingCard,
    reading: Arc<TextReading>,
    selection: Cell<Option<Anchor>>,
    /// Number of the latest selection: the recognition result of an older selection is not shown.
    generation: Cell<u64>,
}

impl Overlay {
    const MIN_SELECTION: f64 = 6.0;

    pub fn show(
        app: &gtk::Application,
        screenshot: Rc<RgbaImage>,
        reading: Arc<TextReading>,
        speech: Arc<ReadingSpeech>,
        activation: &Activation,
    ) -> Rc<ScreenFrame> {
        let frame = ScreenFrame::new(app, screenshot);
        let canvas = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        canvas.set_cursor_from_name(Some("crosshair"));
        let card = ReadingCard::new(speech);
        frame.layers().add_overlay(&canvas);
        frame.layers().add_overlay(card.widget());

        let overlay = Rc::new(Self {
            frame: Rc::clone(&frame),
            canvas: canvas.clone(),
            card,
            reading,
            selection: Cell::new(None),
            generation: Cell::new(0),
        });

        canvas.set_draw_func(glib::clone!(
            #[weak]
            overlay,
            move |_, context, width, height| overlay.draw(context, width, height)
        ));
        frame.layers().connect_get_child_position(glib::clone!(
            #[weak]
            overlay,
            #[upgrade_or]
            None,
            move |layers, widget| overlay.card.position(layers, widget, overlay.selection.get()?)
        ));
        overlay.connect_input();
        // Other handlers hold the overlay weakly; it lives as long as the frame.
        frame.connect_closed(Box::new(move || {
            let _ = &overlay;
        }));

        frame.present(activation);
        frame
    }

    fn connect_input(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = overlay)]
            self,
            move |_, x, y| overlay.begin_selection(x, y)
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = overlay)]
            self,
            move |gesture, offset_x, offset_y| overlay.update_selection(gesture, offset_x, offset_y)
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = overlay)]
            self,
            move |gesture, offset_x, offset_y| overlay.finish_selection(gesture, offset_x, offset_y)
        ));
        self.canvas.add_controller(drag);
    }

    fn begin_selection(&self, x: f64, y: f64) {
        self.card.hide();
        self.selection.set(Some(Anchor::point(x, y)));
        self.canvas.queue_draw();
    }

    fn update_selection(&self, gesture: &gtk::GestureDrag, offset_x: f64, offset_y: f64) {
        if let Some((start_x, start_y)) = gesture.start_point() {
            self.selection.set(Some(Anchor::spanning(start_x, start_y, offset_x, offset_y)));
            self.canvas.queue_draw();
        }
    }

    fn finish_selection(self: &Rc<Self>, gesture: &gtk::GestureDrag, offset_x: f64, offset_y: f64) {
        let Some((start_x, start_y)) = gesture.start_point() else {
            return;
        };
        let area = Anchor::spanning(start_x, start_y, offset_x, offset_y);
        if area.width < Self::MIN_SELECTION || area.height < Self::MIN_SELECTION {
            self.frame.close();
            return;
        }
        self.selection.set(Some(area));
        self.canvas.queue_draw();
        let Some(fragment) = self.fragment(area) else {
            return;
        };

        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.card.show_message(&t!("reading.recognizing"));
        let overlay = Rc::clone(self);
        let reading = Arc::clone(&self.reading);
        let labels_language = InterfaceLanguage::current();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || reading.read_fragment(&fragment, &labels_language)).await;
            if overlay.generation.get() != generation {
                return;
            }
            match result {
                Ok(Ok(reading)) => overlay.card.show_reading(&reading),
                Ok(Err(error)) => {
                    overlay.card.show_message(&t!("reading.recognition_failed", error = format!("{error:#}")))
                }
                Err(_) => overlay.card.show_message(&t!("reading.recognition_crashed")),
            }
        });
    }

    /// Part of the screenshot under the selection, in screenshot pixels.
    fn fragment(&self, area: Anchor) -> Option<RgbaImage> {
        let screen = self.frame.screen();
        let screen = screen.as_ref()?;
        let scale = f64::from(screen.width()) / f64::from(self.canvas.width().max(1));
        let x = (area.x * scale).max(0.0) as u32;
        let y = (area.y * scale).max(0.0) as u32;
        let width = ((area.width * scale) as u32).min(screen.width().saturating_sub(x));
        let height = ((area.height * scale) as u32).min(screen.height().saturating_sub(y));
        (width > 0 && height > 0).then(|| image::imageops::crop_imm(screen, x, y, width, height).to_image())
    }

    fn draw(&self, context: &gtk::cairo::Context, width: i32, height: i32) {
        context.set_source_rgba(0.0, 0.0, 0.0, 0.35);
        context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
        let selection = self.selection.get();
        if let Some(area) = selection {
            context.rectangle(area.x, area.y, area.width, area.height);
        }
        context.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
        let _ = context.fill();

        if let Some(area) = selection {
            context.set_source_rgba(0.21, 0.52, 0.89, 1.0);
            context.set_line_width(2.0);
            context.rectangle(area.x, area.y, area.width, area.height);
            let _ = context.stroke();
        }
    }
}
