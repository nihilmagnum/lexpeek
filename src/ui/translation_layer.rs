use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, pango};
use image::{Rgb, RgbaImage};

use crate::application::screen_translation::TranslatedParagraph;
use crate::domain::text::ScreenArea;

/// Translation drawn over the place of its paragraph in the colors of the original.
struct PaintedParagraph {
    area: ScreenArea,
    font_size: u32,
    /// Number of lines of the original.
    lines: usize,
    translation: String,
    background: gdk::RGBA,
    color: gdk::RGBA,
}

impl PaintedParagraph {
    /// Margin around the paragraph, in screenshot pixels, that covers the antialiased edges of the original letters.
    const MARGIN: u32 = 2;
    /// Share of the strongest difference from the background starting from which a pixel belongs to a letter.
    const LETTER_CONTRAST: f64 = 0.75;

    fn new(screen: &RgbaImage, translated: TranslatedParagraph) -> Self {
        let paragraph = translated.paragraph;
        let area = Self::with_margin(paragraph.area, screen);
        let background = Self::background(screen, area);
        let color = Self::letter_color(screen, paragraph.area, background);
        Self {
            area,
            font_size: paragraph.font_size,
            lines: paragraph.text.lines().count(),
            translation: translated.translation,
            background: Self::rgba(background),
            color: Self::rgba(color),
        }
    }

    fn with_margin(area: ScreenArea, screen: &RgbaImage) -> ScreenArea {
        let (x, y) = (area.x.saturating_sub(Self::MARGIN), area.y.saturating_sub(Self::MARGIN));
        let right = (area.x + area.width + Self::MARGIN).min(screen.width());
        let bottom = (area.bottom() + Self::MARGIN).min(screen.height());
        ScreenArea { x, y, width: right.saturating_sub(x), height: bottom.saturating_sub(y) }
    }

    /// The edge of the area runs between the letters, so the most common color there is the background.
    fn background(screen: &RgbaImage, area: ScreenArea) -> Rgb<u8> {
        let (right, bottom) = (area.x + area.width.max(1) - 1, area.y + area.height.max(1) - 1);
        let horizontal = (area.x..=right).flat_map(|x| [screen.get_pixel(x, area.y), screen.get_pixel(x, bottom)]);
        let vertical = (area.y..=bottom).flat_map(|y| [screen.get_pixel(area.x, y), screen.get_pixel(right, y)]);
        let mut channels: [Vec<u8>; 3] = Default::default();
        for pixel in horizontal.chain(vertical) {
            for (channel, value) in channels.iter_mut().zip(pixel.0) {
                channel.push(value);
            }
        }
        Rgb(channels.map(|mut values| {
            values.sort_unstable();
            values[values.len() / 2]
        }))
    }

    /// Average of the pixels that differ most from the background; black or white if there are none.
    fn letter_color(screen: &RgbaImage, area: ScreenArea, background: Rgb<u8>) -> Rgb<u8> {
        let difference = |pixel: &Rgb<u8>| -> u32 {
            pixel.0.iter().zip(background.0).map(|(value, base)| u32::from(value.abs_diff(base))).sum()
        };
        let pixels: Vec<Rgb<u8>> = (area.y..area.bottom())
            .flat_map(|y| (area.x..area.x + area.width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let [red, green, blue, _] = screen.get_pixel(x, y).0;
                Rgb([red, green, blue])
            })
            .collect();
        let strongest = pixels.iter().map(difference).max().unwrap_or(0);
        let threshold = (f64::from(strongest) * Self::LETTER_CONTRAST) as u32;
        let letters: Vec<&Rgb<u8>> = pixels.iter().filter(|pixel| difference(pixel) >= threshold.max(1)).collect();
        if letters.is_empty() {
            let brightness: u32 = background.0.iter().map(|&value| u32::from(value)).sum();
            return if brightness > 3 * 128 { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) };
        }
        Rgb([0, 1, 2].map(|channel| {
            let sum: usize = letters.iter().map(|pixel| usize::from(pixel.0[channel])).sum();
            (sum / letters.len()) as u8
        }))
    }

    fn rgba(color: Rgb<u8>) -> gdk::RGBA {
        let [red, green, blue] = color.0.map(|value| f32::from(value) / 255.0);
        gdk::RGBA::new(red, green, blue, 1.0)
    }
}

glib::wrapper! {
    /// Translations of the paragraphs of a frozen frame of the screen drawn over their originals.
    pub struct TranslationLayer(ObjectSubclass<imp::TranslationLayer>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TranslationLayer {
    /// Smaller fonts are unreadable.
    const MIN_FONT_SIZE: f64 = 7.0;
    /// The font shrinks at most to this share of the original. A translation that still does not fit grows beyond
    /// the original: a line in width, a paragraph downward.
    const MIN_SCALE: f64 = 0.8;
    const FONT_STEP: f64 = 0.95;

    pub fn new() -> Self {
        glib::Object::builder().property("hexpand", true).property("vexpand", true).build()
    }

    /// `screen` is the frame the paragraphs were recognized on.
    pub fn show(&self, screen: &RgbaImage, paragraphs: Vec<TranslatedParagraph>) {
        let painted = paragraphs.into_iter().map(|paragraph| PaintedParagraph::new(screen, paragraph)).collect();
        self.imp().screen_width.set(screen.width());
        self.imp().paragraphs.replace(painted);
        self.queue_draw();
    }

    fn paint(&self, snapshot: &gtk::Snapshot) {
        let screen_width = self.imp().screen_width.get();
        if screen_width == 0 {
            return;
        }
        let scale = f64::from(self.width()) / f64::from(screen_width);
        for paragraph in self.imp().paragraphs.borrow().iter() {
            let [x, y, width, height] =
                [paragraph.area.x, paragraph.area.y, paragraph.area.width, paragraph.area.height]
                    .map(|value| f64::from(value) * scale);
            let layout = self.fitted_layout(paragraph, width, height, scale);
            let (text_width, text_height) = layout.pixel_size();
            let (text_width, text_height) = (f64::from(text_width), f64::from(text_height));
            // A line stays centered on the original line, a paragraph starts where the original starts.
            let text_top = if paragraph.lines == 1 { y + (height - text_height) / 2.0 } else { y };
            let (top, bottom) = (y.min(text_top), (y + height).max(text_top + text_height));
            let bounds = graphene::Rect::new(x as f32, top as f32, width.max(text_width) as f32, (bottom - top) as f32);
            snapshot.append_color(&paragraph.background, &bounds);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(x as f32, text_top as f32));
            snapshot.append_layout(&layout, &paragraph.color);
            snapshot.restore();
        }
    }

    /// Starts from the size of the original letters and shrinks the font until the translation fits: a line
    /// into the width of the original, a paragraph wrapped at that width into its height.
    fn fitted_layout(&self, paragraph: &PaintedParagraph, width: f64, height: f64, scale: f64) -> pango::Layout {
        let layout = self.create_pango_layout(Some(&paragraph.translation));
        let single_line = paragraph.lines == 1;
        if !single_line {
            layout.set_width((width * f64::from(pango::SCALE)) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        }
        let mut font = self.pango_context().font_description().unwrap_or_default();
        let original = f64::from(paragraph.font_size) * scale;
        let smallest = (original * Self::MIN_SCALE).max(Self::MIN_FONT_SIZE);
        let mut size = original.max(smallest);
        loop {
            font.set_absolute_size(size * f64::from(pango::SCALE));
            layout.set_font_description(Some(&font));
            let (text_width, text_height) = layout.pixel_size();
            let fits =
                if single_line { f64::from(text_width) <= width } else { f64::from(text_height) <= height };
            if fits || size <= smallest {
                return layout;
            }
            size = (size * Self::FONT_STEP).max(smallest);
        }
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::glib;
    use gtk::subclass::prelude::*;

    use super::PaintedParagraph;

    #[derive(Default)]
    pub struct TranslationLayer {
        pub(super) paragraphs: RefCell<Vec<PaintedParagraph>>,
        /// Width of the frame in screenshot pixels: the paragraphs are placed in them.
        pub(super) screen_width: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TranslationLayer {
        const NAME: &'static str = "LexpeekTranslationLayer";
        type Type = super::TranslationLayer;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for TranslationLayer {}

    impl WidgetImpl for TranslationLayer {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().paint(snapshot);
        }
    }
}
