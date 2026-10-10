use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use anyhow::{Context, Result};
use gtk::prelude::*;
use gtk::{gio, glib};

/// Sound playback with GTK, that is, through GStreamer.
pub struct GtkAudioPlayer;

impl GtkAudioPlayer {
    /// Returns when the sound has finished. GTK objects live only on the main thread, so call it
    /// from a background thread: called from the main thread it would never see the end.
    pub fn play(&self, audio: Vec<u8>) -> Result<()> {
        let (finished, result) = mpsc::channel::<Result<()>>();
        glib::MainContext::default().invoke(move || Self::start(audio, finished));
        result.recv().context("audio playback was interrupted")?
    }

    fn start(audio: Vec<u8>, finished: mpsc::Sender<Result<()>>) {
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(audio));
        let media = gtk::MediaFile::for_input_stream(&stream);
        // The player is held here until the sound finishes; it is released outside the handler of its own
        // signal, otherwise it would be destroyed in the middle of that signal.
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
