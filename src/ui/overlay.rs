use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use image::RgbaImage;

use crate::application::reading::TextReading;
use crate::application::speech::ReadingSpeech;
use crate::ui::action::Activation;
use crate::ui::reading_card::{Anchor, ReadingCard};

/// Frozen frame of the screen on which text is selected for translation.
pub struct Overlay {
    window: gtk::ApplicationWindow,
    picture: gtk::Picture,
    canvas: gtk::DrawingArea,
    card: ReadingCard,
    screenshot: RgbaImage,
    reading: Arc<TextReading>,
    /// Screenshot of the monitor the frame is open on.
    screen: RefCell<Option<RgbaImage>>,
    selection: Cell<Option<Anchor>>,
    /// Number of the latest selection: the recognition result of an older selection is not shown.
    generation: Cell<u64>,
}

impl Overlay {
    const MIN_SELECTION: f64 = 6.0;

    pub fn show(
        app: &gtk::Application,
        screenshot: RgbaImage,
        reading: Arc<TextReading>,
        speech: Arc<ReadingSpeech>,
        activation: &Activation,
    ) {
        let window = gtk::ApplicationWindow::builder().application(app).decorated(false).build();
        let picture = gtk::Picture::builder().content_fit(gtk::ContentFit::Fill).can_shrink(true).build();
        let canvas = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        canvas.set_cursor_from_name(Some("crosshair"));
        let card = ReadingCard::new(speech);

        let layers = gtk::Overlay::builder().child(&picture).build();
        layers.add_overlay(&canvas);
        layers.add_overlay(card.widget());
        window.set_child(Some(&layers));

        let overlay = Rc::new(Self {
            window: window.clone(),
            picture,
            canvas: canvas.clone(),
            card,
            screenshot,
            reading,
            screen: RefCell::new(None),
            selection: Cell::new(None),
            generation: Cell::new(0),
        });

        canvas.set_draw_func(glib::clone!(
            #[weak]
            overlay,
            move |_, context, width, height| overlay.draw(context, width, height)
        ));
        layers.connect_get_child_position(glib::clone!(
            #[weak]
            overlay,
            #[upgrade_or]
            None,
            move |layers, widget| overlay.card.position(layers, widget, overlay.selection.get()?)
        ));
        window.connect_realize(glib::clone!(
            #[weak]
            overlay,
            move |window| {
                if let Some(surface) = window.surface() {
                    surface.connect_enter_monitor(glib::clone!(
                        #[weak]
                        overlay,
                        move |_, monitor| overlay.show_monitor(monitor)
                    ));
                }
            }
        ));
        overlay.connect_input();
        // Other handlers hold the overlay weakly; it lives as long as the window.
        window.connect_destroy(move |_| {
            let _ = &overlay;
        });

        window.fullscreen();
        activation.present(&window);
    }

    fn connect_input(self: &Rc<Self>) {
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

    /// The portal captures all monitors at once, while the window occupies one of them.
    fn show_monitor(&self, current: &gdk::Monitor) {
        let display = WidgetExt::display(&self.window);
        let monitors: Vec<gdk::Monitor> = display.monitors().iter::<gdk::Monitor>().filter_map(Result::ok).collect();
        let screen = Self::crop_to_monitor(&self.screenshot, &monitors, current);
        let texture = gdk::MemoryTexture::new(
            screen.width() as i32,
            screen.height() as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from(screen.as_raw()),
            screen.width() as usize * 4,
        );
        self.picture.set_paintable(Some(&texture));
        self.screen.replace(Some(screen));
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
            self.window.close();
            return;
        }
        self.selection.set(Some(area));
        self.canvas.queue_draw();
        let Some(fragment) = self.fragment(area) else {
            return;
        };

        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.card.show_message("Распознаю…");
        let overlay = Rc::clone(self);
        let reading = Arc::clone(&self.reading);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || reading.read_fragment(&fragment)).await;
            if overlay.generation.get() != generation {
                return;
            }
            match result {
                Ok(Ok(reading)) => overlay.card.show_reading(&reading),
                Ok(Err(error)) => overlay.card.show_message(&format!("Не удалось распознать текст: {error:#}")),
                Err(_) => overlay.card.show_message("Распознавание аварийно завершилось"),
            }
        });
    }

    /// Part of the screenshot under the selection, in screenshot pixels.
    fn fragment(&self, area: Anchor) -> Option<RgbaImage> {
        let screen = self.screen.borrow();
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

    fn crop_to_monitor(screenshot: &RgbaImage, monitors: &[gdk::Monitor], current: &gdk::Monitor) -> RgbaImage {
        let bounds = monitors
            .iter()
            .map(|monitor| monitor.geometry())
            .reduce(|union, geometry| union.union(&geometry))
            .unwrap_or_else(|| current.geometry());
        let scale = f64::from(screenshot.width()) / f64::from(bounds.width().max(1));
        let geometry = current.geometry();
        let x = (f64::from(geometry.x() - bounds.x()) * scale) as u32;
        let y = (f64::from(geometry.y() - bounds.y()) * scale) as u32;
        let width = ((f64::from(geometry.width()) * scale) as u32).min(screenshot.width().saturating_sub(x));
        let height = ((f64::from(geometry.height()) * scale) as u32).min(screenshot.height().saturating_sub(y));
        image::imageops::crop_imm(screenshot, x, y, width, height).to_image()
    }
}
