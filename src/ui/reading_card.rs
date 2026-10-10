use gtk::prelude::*;
use std::sync::Arc;

use gtk::{gdk, gio, glib, pango};
use rust_i18n::t;

use crate::application::reading::Reading;
use crate::application::speech::ReadingSpeech;
use crate::domain::dictionary::{Definition, DictionaryArticle, Meaning};
use crate::domain::text::Phrase;

/// Rectangle on the screen next to which the card is placed.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Anchor {
    pub fn point(x: f64, y: f64) -> Self {
        Self { x, y, width: 0.0, height: 0.0 }
    }

    pub fn spanning(start_x: f64, start_y: f64, offset_x: f64, offset_y: f64) -> Self {
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

/// Card with the recognized text, its translation and the dictionary article.
pub struct ReadingCard {
    card: gtk::ScrolledWindow,
    speech: Arc<ReadingSpeech>,
}

impl ReadingCard {
    const WIDTH: i32 = 440;
    const GAP: i32 = 10;
    const TRANSLATIONS_PER_PART: usize = 6;
    const BACK_TRANSLATIONS: usize = 4;
    const DEFINITIONS_PER_PART: usize = 3;
    const SYNONYMS: usize = 6;
    const EXAMPLES: usize = 3;

    pub fn new(speech: Arc<ReadingSpeech>) -> Self {
        let card = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(520)
            .width_request(Self::WIDTH)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .visible(false)
            .css_classes(["lexpeek-card"])
            .build();
        Self { card, speech }
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.card
    }

    pub fn hide(&self) {
        self.card.set_visible(false);
    }

    pub fn show_message(&self, text: &str) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&Self::label(text, &[]));
        self.show(&content);
    }

    pub fn show_reading(&self, reading: &Reading) {
        if reading.text.is_empty() {
            return self.show_message(&t!("reading.nothing_recognized"));
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let article = reading.translation.as_ref().ok().map(|translation| &translation.article);
        let transcription = article.and_then(|article| article.transcription.as_deref());
        let original = Self::original_label(&reading.text, transcription);
        content.append(&self.spoken_view(&original, reading.original(), t!("reading.speak_original").into()));
        match &reading.translation {
            Ok(translation) => {
                let translated = Self::label(&translation.text, &["lexpeek-translation"]);
                let tooltip = t!("reading.speak_translation").into();
                content.append(&self.spoken_view(&translated, Some(translation.phrase()), tooltip));
                Self::append_article(&content, &translation.article);
            }
            Err(error) => {
                content.append(&Self::label(&t!("reading.translation_failed", error = format!("{error:#}")), &[]))
            }
        }
        self.show(&content);
    }

    /// Position of the card in `layers`: below the anchor if there is room, otherwise above it.
    pub fn position(&self, layers: &gtk::Overlay, widget: &gtk::Widget, anchor: Anchor) -> Option<gdk::Rectangle> {
        if widget != self.card.upcast_ref::<gtk::Widget>() {
            return None;
        }
        let (_, natural) = widget.preferred_size();
        let height = natural.height();
        let (screen_width, screen_height) = (layers.width(), layers.height());

        let x = (anchor.x as i32).clamp(Self::GAP, (screen_width - Self::WIDTH - Self::GAP).max(Self::GAP));
        let below = anchor.bottom() as i32 + Self::GAP;
        let above = anchor.y as i32 - Self::GAP - height;
        let y = if below + height <= screen_height {
            below
        } else if above >= Self::GAP {
            above
        } else {
            (screen_height - height - Self::GAP).max(Self::GAP)
        };
        Some(gdk::Rectangle::new(x, y, Self::WIDTH, height))
    }

    fn show(&self, content: &gtk::Box) {
        self.card.set_child(Some(content));
        self.card.set_visible(true);
        self.card.vadjustment().set_value(0.0);
    }

    /// Text with a speech button, if there is something to speak.
    fn spoken_view(&self, text: &gtk::Label, phrase: Option<Phrase>, tooltip: String) -> gtk::Box {
        let view = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        text.set_hexpand(true);
        view.append(text);
        if let Some(phrase) = phrase {
            view.append(&self.speak_button(phrase, tooltip));
        }
        view
    }

    fn speak_button(&self, phrase: Phrase, tooltip: String) -> gtk::Button {
        let button = gtk::Button::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text(&tooltip)
            .valign(gtk::Align::Start)
            .css_classes(["flat"])
            .build();
        let speech = Arc::clone(&self.speech);
        button.connect_clicked(move |button| Self::speak(button, Arc::clone(&speech), phrase.clone(), tooltip.clone()));
        button
    }

    fn speak(button: &gtk::Button, speech: Arc<ReadingSpeech>, phrase: Phrase, tooltip: String) {
        button.set_sensitive(false);
        glib::spawn_future_local(glib::clone!(
            #[weak]
            button,
            async move {
                let spoken = gio::spawn_blocking(move || speech.speak(&phrase)).await;
                button.set_sensitive(true);
                let failure = match spoken {
                    Ok(Ok(())) => return button.set_tooltip_text(Some(&tooltip)),
                    Ok(Err(error)) => t!("reading.speech_failed", error = format!("{error:#}")),
                    Err(_) => t!("reading.speech_crashed"),
                };
                button.set_tooltip_text(Some(&failure));
            }
        ));
    }

    fn original_label(text: &str, transcription: Option<&str>) -> gtk::Label {
        let mut markup = format!("<i>{}</i>", glib::markup_escape_text(text));
        if let Some(transcription) = transcription {
            markup.push_str(&format!("  <span alpha=\"70%\">[{}]</span>", glib::markup_escape_text(transcription)));
        }
        Self::markup_label(&markup, &["lexpeek-recognized"])
    }

    fn append_article(content: &gtk::Box, article: &DictionaryArticle) {
        if article.is_empty() {
            return;
        }
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        for meaning in &article.meanings {
            content.append(&Self::meaning_view(meaning));
        }
        if !article.definitions.is_empty() {
            content.append(&Self::label(&t!("reading.definitions"), &["lexpeek-section"]));
            let mut parts: Vec<&str> = Vec::new();
            for definition in &article.definitions {
                if !parts.contains(&definition.part_of_speech.as_str()) {
                    parts.push(&definition.part_of_speech);
                }
            }
            for part in parts {
                content.append(&Self::label(part, &["lexpeek-part-of-speech"]));
                let definitions = article.definitions.iter().filter(|definition| definition.part_of_speech == part);
                for definition in definitions.take(Self::DEFINITIONS_PER_PART) {
                    content.append(&Self::definition_view(definition));
                }
            }
        }
        if !article.examples.is_empty() {
            content.append(&Self::label(&t!("reading.examples"), &["lexpeek-section"]));
            for example in article.examples.iter().take(Self::EXAMPLES) {
                content.append(&Self::markup_label(&format!("• <i>{}</i>", glib::markup_escape_text(example)), &[]));
            }
        }
    }

    fn meaning_view(meaning: &Meaning) -> gtk::Box {
        let view = gtk::Box::new(gtk::Orientation::Vertical, 2);
        view.append(&Self::label(&meaning.part_of_speech, &["lexpeek-part-of-speech"]));
        for translation in meaning.translations.iter().take(Self::TRANSLATIONS_PER_PART) {
            let back: Vec<&str> =
                translation.back_translations.iter().take(Self::BACK_TRANSLATIONS).map(String::as_str).collect();
            let mut markup = format!("<b>{}</b>", glib::markup_escape_text(&translation.word));
            for synonym in &translation.synonyms {
                markup.push_str(&format!(", {}", glib::markup_escape_text(synonym)));
            }
            markup.push_str(&format!("  <span alpha=\"65%\">{}</span>", glib::markup_escape_text(&back.join(", "))));
            view.append(&Self::markup_label(&markup, &[]));
        }
        view
    }

    fn definition_view(definition: &Definition) -> gtk::Box {
        let view = gtk::Box::new(gtk::Orientation::Vertical, 2);
        view.append(&Self::label(&definition.gloss, &[]));
        if let Some(example) = &definition.example {
            let quoted = t!("reading.quoted_example", example = glib::markup_escape_text(example));
            let markup = format!("<i>{quoted}</i>");
            view.append(&Self::markup_label(&markup, &["lexpeek-dim"]));
        }
        if !definition.synonyms.is_empty() {
            let synonyms: Vec<&str> = definition.synonyms.iter().take(Self::SYNONYMS).map(String::as_str).collect();
            view.append(&Self::label(&t!("reading.synonyms", synonyms = synonyms.join(", ")), &["lexpeek-dim"]));
        }
        view
    }

    fn markup_label(markup: &str, classes: &[&str]) -> gtk::Label {
        let label = Self::label("", classes);
        label.set_markup(markup);
        label
    }

    fn label(text: &str, classes: &[&str]) -> gtk::Label {
        gtk::Label::builder()
            .label(text)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .css_classes(classes.iter().copied().map(String::from).collect::<Vec<_>>())
            .build()
    }
}
