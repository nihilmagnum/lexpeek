use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use anyhow::{Context, Result};
use gtk::prelude::*;
use gtk::{gio, glib};

/// Проигрывание звука средствами GTK, то есть через GStreamer.
pub struct GtkAudioPlayer;

impl GtkAudioPlayer {
    /// Возвращается, когда звук доиграл. Объекты GTK живут только в главном потоке, поэтому
    /// вызывать из фонового потока: из главного вызов никогда не дождётся конца.
    pub fn play(&self, audio: Vec<u8>) -> Result<()> {
        let (finished, result) = mpsc::channel::<Result<()>>();
        glib::MainContext::default().invoke(move || Self::start(audio, finished));
        result.recv().context("audio playback was interrupted")?
    }

    fn start(audio: Vec<u8>, finished: mpsc::Sender<Result<()>>) {
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(audio));
        let media = gtk::MediaFile::for_input_stream(&stream);
        // Проигрыватель держится здесь, пока звук не доиграет; отпускается он вне обработчика его же
        // сигнала, иначе уничтожался бы посреди этого сигнала.
        let playing = Rc::new(RefCell::new(Some(media.clone())));
        media.connect_notify_local(None, move |media, _| {
            let outcome = match media.error() {
                Some(error) => Err(error.into()),
                None if media.is_ended() => Ok(()),
                None => return,
            };
            let Some(player) = playing.borrow_mut().take() else {
                return;
            };
            let _ = finished.send(outcome);
            glib::idle_add_local_once(move || drop(player));
        });
        media.play();
    }
}
