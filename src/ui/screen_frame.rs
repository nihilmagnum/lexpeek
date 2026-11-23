use std::cell::{Ref, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};
use image::RgbaImage;

use crate::ui::action::Activation;
use crate::ui::escape;
use crate::ui::frozen_screen::Frame;

/// Fullscreen window with a frozen frame of the monitor it is open on; Esc closes it.
pub struct ScreenFrame {
    window: gtk::ApplicationWindow,
    layers: gtk::Overlay,
    picture: gtk::Picture,
    /// Screenshot of all monitors the frame was made from.
    screenshot: Rc<RgbaImage>,
    /// Screenshot of the monitor the frame is open on.
    screen: RefCell<Option<RgbaImage>>,
    on_screen: RefCell<Option<Box<dyn Fn(&RgbaImage)>>>,
}

impl ScreenFrame {
    pub fn new(app: &gtk::Application, screenshot: Rc<RgbaImage>) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder().application(app).decorated(false).build();
        let picture = gtk::Picture::builder().content_fit(gtk::ContentFit::Fill).can_shrink(true).build();
        let layers = gtk::Overlay::builder().child(&picture).build();
        window.set_child(Some(&layers));

        let frame = Rc::new(Self {
            window: window.clone(),
            layers,
            picture,
            screenshot,
            screen: RefCell::new(None),
            on_screen: RefCell::new(None),
        });
        window.connect_realize(glib::clone!(
            #[weak]
            frame,
            move |window| {
                if let Some(surface) = window.surface() {
                    surface.connect_enter_monitor(glib::clone!(
                        #[weak]
                        frame,
                        move |_, monitor| frame.show_monitor(monitor)
                    ));
                }
            }
        ));
        escape::closes_window(window.upcast_ref());
        frame
    }

    pub fn window(&self) -> &gtk::ApplicationWindow {
        &self.window
    }

    /// Container for what is shown over the frame.
    pub fn layers(&self) -> &gtk::Overlay {
        &self.layers
    }

    pub fn screen(&self) -> Ref<'_, Option<RgbaImage>> {
        self.screen.borrow()
    }

    /// Calls `show` with the screenshot of the monitor once the window is on one.
    pub fn connect_screen(&self, show: impl Fn(&RgbaImage) + 'static) {
        self.on_screen.replace(Some(Box::new(show)));
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
        if let (Some(show), Some(screen)) = (self.on_screen.borrow().as_ref(), self.screen.borrow().as_ref()) {
            show(screen);
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

impl Frame for ScreenFrame {
    fn present(&self, activation: &Activation) {
        self.window.fullscreen();
        activation.present(&self.window);
    }

    fn close(&self) {
        self.window.close();
    }

    fn connect_closed(&self, closed: Box<dyn Fn()>) {
        self.window.connect_destroy(move |_| closed());
    }
}
