use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use image::RgbaImage;

use crate::application::reading::{Reading, ScreenReading};
use crate::domain::lookup::TermMatch;

#[derive(Clone, Copy, Debug)]
struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Area {
    fn spanning(start_x: f64, start_y: f64, offset_x: f64, offset_y: f64) -> Self {
        Self {
            x: start_x.min(start_x + offset_x),
            y: start_y.min(start_y + offset_y),
            width: offset_x.abs(),
            height: offset_y.abs(),
        }
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Замороженный кадр экрана, на котором выделяют текст для перевода.
pub struct Overlay {
    window: gtk::ApplicationWindow,
    picture: gtk::Picture,
    canvas: gtk::DrawingArea,
    card: gtk::ScrolledWindow,
    screenshot: RgbaImage,
    reading: Arc<ScreenReading>,
    /// Снимок монитора, на котором открыт кадр.
    screen: RefCell<Option<RgbaImage>>,
    selection: RefCell<Option<Area>>,
    /// Номер последнего выделения: результат распознавания старого выделения не показывается.
    generation: Cell<u64>,
}

impl Overlay {
    const CARD_WIDTH: i32 = 440;
    const CARD_GAP: i32 = 10;
    const MIN_SELECTION: f64 = 6.0;

    pub fn show(app: &gtk::Application, screenshot: RgbaImage, reading: Arc<ScreenReading>) {
        let window = gtk::ApplicationWindow::builder().application(app).decorated(false).build();
        let picture = gtk::Picture::builder().content_fit(gtk::ContentFit::Fill).can_shrink(true).build();
        let canvas = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        canvas.set_cursor_from_name(Some("crosshair"));
        let card = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .width_request(Self::CARD_WIDTH)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .visible(false)
            .css_classes(["lexpeek-card"])
            .build();

        let layers = gtk::Overlay::builder().child(&picture).build();
        layers.add_overlay(&canvas);
        layers.add_overlay(&card);
        window.set_child(Some(&layers));

        let overlay = Rc::new(Self {
            window: window.clone(),
            picture,
            canvas: canvas.clone(),
            card,
            screenshot,
            reading,
            screen: RefCell::new(None),
            selection: RefCell::new(None),
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
            move |layers, widget| overlay.card_position(layers, widget)
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
        // Остальные обработчики держат оверлей слабой ссылкой, живёт он столько же, сколько окно.
        window.connect_destroy(move |_| {
            let _ = &overlay;
        });

        window.fullscreen();
        window.present();
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

    /// Портал снимает все мониторы разом, а окно занимает один из них.
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
        self.card.set_visible(false);
        self.selection.replace(Some(Area { x, y, width: 0.0, height: 0.0 }));
        self.canvas.queue_draw();
    }

    fn update_selection(&self, gesture: &gtk::GestureDrag, offset_x: f64, offset_y: f64) {
        if let Some((start_x, start_y)) = gesture.start_point() {
            self.selection.replace(Some(Area::spanning(start_x, start_y, offset_x, offset_y)));
            self.canvas.queue_draw();
        }
    }

    fn finish_selection(self: &Rc<Self>, gesture: &gtk::GestureDrag, offset_x: f64, offset_y: f64) {
        let Some((start_x, start_y)) = gesture.start_point() else {
            return;
        };
        let area = Area::spanning(start_x, start_y, offset_x, offset_y);
        if area.width < Self::MIN_SELECTION || area.height < Self::MIN_SELECTION {
            self.window.close();
            return;
        }
        self.selection.replace(Some(area));
        self.canvas.queue_draw();
        let Some(fragment) = self.fragment(area) else {
            return;
        };

        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.show_card(&Self::message("Распознаю…"));
        let overlay = Rc::clone(self);
        let reading = Arc::clone(&self.reading);
        glib::spawn_future_local(async move {
            let reading = gio::spawn_blocking(move || reading.read(&fragment)).await;
            if overlay.generation.get() != generation {
                return;
            }
            let content = match reading {
                Ok(Ok(reading)) => Self::reading_content(&reading),
                Ok(Err(error)) => Self::message(&format!("Не удалось распознать текст: {error:#}")),
                Err(_) => Self::message("Распознавание аварийно завершилось"),
            };
            overlay.show_card(&content);
        });
    }

    /// Участок снимка под выделением в пикселях снимка.
    fn fragment(&self, area: Area) -> Option<RgbaImage> {
        let screen = self.screen.borrow();
        let screen = screen.as_ref()?;
        let scale = f64::from(screen.width()) / f64::from(self.canvas.width().max(1));
        let x = (area.x * scale).max(0.0) as u32;
        let y = (area.y * scale).max(0.0) as u32;
        let width = ((area.width * scale) as u32).min(screen.width().saturating_sub(x));
        let height = ((area.height * scale) as u32).min(screen.height().saturating_sub(y));
        (width > 0 && height > 0).then(|| image::imageops::crop_imm(screen, x, y, width, height).to_image())
    }

    fn reading_content(reading: &Reading) -> gtk::Box {
        if reading.text.is_empty() {
            return Self::message("Текст не распознан");
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.append(&Self::label(&reading.text, &["lexpeek-recognized"]));
        match &reading.translation {
            Ok(translation) => content.append(&Self::label(translation, &["lexpeek-translation"])),
            Err(error) => content.append(&Self::label(&format!("Не удалось перевести: {error:#}"), &[])),
        }
        match &reading.terms {
            Ok(terms) if terms.is_empty() => {}
            Ok(terms) => {
                content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                for term in terms {
                    content.append(&Self::term_view(term));
                }
            }
            Err(error) => content.append(&Self::label(&format!("Ошибка словаря: {error:#}"), &[])),
        }
        content
    }

    fn term_view(term: &TermMatch) -> gtk::Box {
        let view = gtk::Box::new(gtk::Orientation::Vertical, 4);
        view.append(&Self::label(&term.term, &["lexpeek-term"]));
        for article in &term.articles {
            view.append(&Self::label(&article.body, &[]));
            view.append(&Self::label(&article.dictionary, &["lexpeek-source"]));
        }
        view
    }

    fn message(text: &str) -> gtk::Box {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&Self::label(text, &[]));
        content
    }

    fn label(text: &str, classes: &[&str]) -> gtk::Label {
        gtk::Label::builder()
            .label(text)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .css_classes(classes.iter().copied().map(String::from).collect::<Vec<_>>())
            .build()
    }

    fn show_card(&self, content: &gtk::Box) {
        self.card.set_child(Some(content));
        self.card.set_visible(true);
        self.card.vadjustment().set_value(0.0);
    }

    /// Карточка встаёт под выделением, если там хватает места, иначе над ним.
    fn card_position(&self, layers: &gtk::Overlay, widget: &gtk::Widget) -> Option<gdk::Rectangle> {
        if widget != self.card.upcast_ref::<gtk::Widget>() {
            return None;
        }
        let area = (*self.selection.borrow())?;
        let (_, natural) = widget.preferred_size();
        let (width, height) = (Self::CARD_WIDTH, natural.height());
        let (screen_width, screen_height) = (layers.width(), layers.height());

        let x = (area.x as i32).clamp(Self::CARD_GAP, (screen_width - width - Self::CARD_GAP).max(Self::CARD_GAP));
        let below = area.bottom() as i32 + Self::CARD_GAP;
        let above = area.y as i32 - Self::CARD_GAP - height;
        let y = if below + height <= screen_height {
            below
        } else if above >= Self::CARD_GAP {
            above
        } else {
            (screen_height - height - Self::CARD_GAP).max(Self::CARD_GAP)
        };
        Some(gdk::Rectangle::new(x, y, width, height))
    }

    fn draw(&self, context: &gtk::cairo::Context, width: i32, height: i32) {
        context.set_source_rgba(0.0, 0.0, 0.0, 0.35);
        context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
        let selection = *self.selection.borrow();
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
