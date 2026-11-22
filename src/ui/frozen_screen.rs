use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use gtk::{gio, glib};
use image::RgbaImage;

use crate::application::desktop::ScreenCapture;
use crate::ui::action::Activation;

/// Mode in which the frozen screen is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMode {
    Area,
    WholeScreen,
}

/// Window of the frozen screen in one mode.
pub trait Frame {
    fn present(&self, activation: &Activation);
    fn close(&self);
    fn connect_closed(&self, closed: Box<dyn Fn()>);
}

type Capture = Pin<Box<dyn Future<Output = Result<RgbaImage>>>>;
/// Opening of a frame in a mode over a screenshot, with the focus given to it.
type ShowFrame = dyn Fn(ScreenMode, Rc<RgbaImage>, &Activation) -> Rc<dyn Frame>;

/// Screen frozen in a screenshot and open in one mode at a time.
pub struct FrozenScreen {
    capture_screen: Box<dyn Fn() -> Capture>,
    show_frame: Box<ShowFrame>,
    state: RefCell<State>,
}

enum State {
    Idle,
    /// The latest press chooses the mode and gets the focus.
    Capturing { mode: ScreenMode, activation: Activation },
    Open(OpenFrame),
}

#[derive(Clone)]
struct OpenFrame {
    mode: ScreenMode,
    frame: Rc<dyn Frame>,
    screenshot: Rc<RgbaImage>,
}

impl FrozenScreen {
    pub fn new(
        screen_capture: Arc<dyn ScreenCapture>,
        show_frame: impl Fn(ScreenMode, Rc<RgbaImage>, &Activation) -> Rc<dyn Frame> + 'static,
    ) -> Rc<Self> {
        let capture_screen = move || -> Capture {
            let screen_capture = Arc::clone(&screen_capture);
            Box::pin(async move {
                let captured = gio::spawn_blocking(move || screen_capture.capture()).await;
                captured.unwrap_or_else(|_| Err(anyhow!("screen capture crashed")))
            })
        };
        Self::with_capture(capture_screen, Box::new(show_frame))
    }

    /// Tests capture the screen on the main context, without a thread pool.
    fn with_capture(capture_screen: impl Fn() -> Capture + 'static, show_frame: Box<ShowFrame>) -> Rc<Self> {
        Rc::new(Self { capture_screen: Box::new(capture_screen), show_frame, state: RefCell::new(State::Idle) })
    }

    /// Shows the frozen screen in the mode; the screen is captured only when no frame is open or opening.
    pub fn open(self: &Rc<Self>, mode: ScreenMode, activation: Activation) {
        // The branches below write the state, some through a closing frame, so it must not stay borrowed.
        let open = match &mut *self.state.borrow_mut() {
            State::Idle => None,
            State::Capturing { mode: pending_mode, activation: pending_activation } => {
                *pending_mode = mode;
                *pending_activation = activation;
                return;
            }
            State::Open(open) => Some(open.clone()),
        };
        match open {
            None => self.capture(mode, activation),
            Some(open) if open.mode == mode => open.frame.present(&activation),
            Some(open) => {
                self.show(mode, open.screenshot, &activation);
                open.frame.close();
            }
        }
    }

    fn capture(self: &Rc<Self>, mode: ScreenMode, activation: Activation) {
        self.state.replace(State::Capturing { mode, activation });
        let capture = (self.capture_screen)();
        let frozen_screen = Rc::clone(self);
        glib::spawn_future_local(async move {
            let captured = capture.await;
            let State::Capturing { mode, activation } = frozen_screen.state.replace(State::Idle) else {
                unreachable!("only a finished capture leaves the capturing state");
            };
            match captured {
                Ok(screenshot) => frozen_screen.show(mode, Rc::new(screenshot), &activation),
                Err(error) => eprintln!("lexpeek: {error:#}"),
            }
        });
    }

    fn show(self: &Rc<Self>, mode: ScreenMode, screenshot: Rc<RgbaImage>, activation: &Activation) {
        let frame = (self.show_frame)(mode, Rc::clone(&screenshot), activation);
        let frozen_screen = Rc::downgrade(self);
        let closed_frame = Rc::downgrade(&frame);
        frame.connect_closed(Box::new(move || {
            if let Some(frozen_screen) = frozen_screen.upgrade() {
                frozen_screen.forget(&closed_frame);
            }
        }));
        self.state.replace(State::Open(OpenFrame { mode, frame, screenshot }));
    }

    /// A replaced frame closes when the new one is already open, and the new one stays.
    fn forget(&self, closed_frame: &Weak<dyn Frame>) {
        let is_open =
            matches!(&*self.state.borrow(), State::Open(open) if Rc::downgrade(&open.frame).ptr_eq(closed_frame));
        if is_open {
            self.state.replace(State::Idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum FrameEvent {
        Shown(ScreenMode),
        Closed(ScreenMode),
    }

    struct FakeFrame {
        mode: ScreenMode,
        screenshot: Rc<RgbaImage>,
        activation: Activation,
        presented: RefCell<Vec<Activation>>,
        events: Rc<RefCell<Vec<FrameEvent>>>,
        closed: RefCell<Vec<Box<dyn Fn()>>>,
    }

    impl Frame for FakeFrame {
        fn present(&self, activation: &Activation) {
            self.presented.borrow_mut().push(activation.clone());
        }

        /// Like a GTK window, calls the handlers before returning.
        fn close(&self) {
            self.events.borrow_mut().push(FrameEvent::Closed(self.mode));
            for closed in self.closed.borrow().iter() {
                closed();
            }
        }

        fn connect_closed(&self, closed: Box<dyn Fn()>) {
            self.closed.borrow_mut().push(closed);
        }
    }

    struct Desktop {
        context: glib::MainContext,
        /// Captures in the order they started; each one waits until the test finishes it.
        captures: Rc<RefCell<Vec<async_channel::Sender<Result<RgbaImage>>>>>,
        frames: Rc<RefCell<Vec<Rc<FakeFrame>>>>,
        events: Rc<RefCell<Vec<FrameEvent>>>,
        frozen_screen: Rc<FrozenScreen>,
    }

    impl Desktop {
        fn new() -> Self {
            let captures = Rc::new(RefCell::new(Vec::new()));
            let started = Rc::clone(&captures);
            let capture_screen = move || -> Capture {
                let (finish, finished) = async_channel::bounded(1);
                started.borrow_mut().push(finish);
                Box::pin(async move { finished.recv().await? })
            };
            let frames = Rc::new(RefCell::new(Vec::new()));
            let events = Rc::new(RefCell::new(Vec::new()));
            let (shown, frame_events) = (Rc::clone(&frames), Rc::clone(&events));
            let show_frame = move |mode, screenshot, activation: &Activation| -> Rc<dyn Frame> {
                frame_events.borrow_mut().push(FrameEvent::Shown(mode));
                let frame = Rc::new(FakeFrame {
                    mode,
                    screenshot,
                    activation: activation.clone(),
                    presented: RefCell::default(),
                    events: Rc::clone(&frame_events),
                    closed: RefCell::default(),
                });
                shown.borrow_mut().push(Rc::clone(&frame));
                frame
            };
            let frozen_screen = FrozenScreen::with_capture(capture_screen, Box::new(show_frame));
            Self { context: glib::MainContext::new(), captures, frames, events, frozen_screen }
        }

        /// Runs the test with the main context on which the frozen screen waits for captures.
        fn run(test: impl FnOnce(&Self)) {
            let desktop = Self::new();
            desktop.context.with_thread_default(|| test(&desktop)).unwrap();
        }

        fn press(&self, mode: ScreenMode, token: &str) {
            self.frozen_screen.open(mode, activation(token));
        }

        fn captures_started(&self) -> usize {
            self.captures.borrow().len()
        }

        /// The screenshot is `width` pixels wide, so a frame shows which capture it was made from.
        fn finish_capture(&self, index: usize, width: u32) {
            self.finish(index, Ok(RgbaImage::new(width, 1)));
        }

        fn fail_capture(&self, index: usize) {
            self.finish(index, Err(anyhow!("screen capture failed")));
        }

        fn finish(&self, index: usize, captured: Result<RgbaImage>) {
            self.captures.borrow()[index].try_send(captured).unwrap();
            while self.context.iteration(false) {}
        }

        fn frames_shown(&self) -> usize {
            self.frames.borrow().len()
        }

        fn frame(&self, index: usize) -> Rc<FakeFrame> {
            Rc::clone(&self.frames.borrow()[index])
        }
    }

    fn activation(token: &str) -> Activation {
        Activation::with_token(Some(token.to_owned()))
    }

    #[test]
    fn repeated_press_during_capture_opens_one_frame() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "first");
            desktop.press(ScreenMode::Area, "second");
            desktop.finish_capture(0, 1);

            assert_eq!(desktop.captures_started(), 1);
            assert_eq!(desktop.frames_shown(), 1);
        });
    }

    #[test]
    fn latest_press_during_capture_chooses_mode_and_focus() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "area");
            desktop.press(ScreenMode::WholeScreen, "whole screen");
            desktop.finish_capture(0, 1);

            let frame = desktop.frame(0);
            assert_eq!(frame.mode, ScreenMode::WholeScreen);
            assert_eq!(frame.activation, activation("whole screen"));
        });
    }

    #[test]
    fn same_mode_presents_open_frame() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "open");
            desktop.finish_capture(0, 1);
            desktop.press(ScreenMode::Area, "present");

            assert_eq!(*desktop.frame(0).presented.borrow(), [activation("present")]);
            assert_eq!(desktop.frames_shown(), 1);
            assert_eq!(desktop.captures_started(), 1);
        });
    }

    #[test]
    fn other_mode_replaces_frame_over_same_screenshot() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "area");
            desktop.finish_capture(0, 1);
            desktop.press(ScreenMode::WholeScreen, "whole screen");
            desktop.press(ScreenMode::WholeScreen, "present");

            let (replaced, replacing) = (desktop.frame(0), desktop.frame(1));
            assert_eq!(
                *desktop.events.borrow(),
                [
                    FrameEvent::Shown(ScreenMode::Area),
                    FrameEvent::Shown(ScreenMode::WholeScreen),
                    FrameEvent::Closed(ScreenMode::Area),
                ]
            );
            assert!(Rc::ptr_eq(&replacing.screenshot, &replaced.screenshot));
            assert_eq!(*replacing.presented.borrow(), [activation("present")]);
            assert_eq!(desktop.captures_started(), 1);
        });
    }

    #[test]
    fn closed_frame_is_captured_anew() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "first");
            desktop.finish_capture(0, 1);
            desktop.frame(0).close();
            desktop.press(ScreenMode::Area, "second");
            desktop.finish_capture(1, 2);

            assert_eq!(desktop.frame(1).screenshot.width(), 2);
        });
    }

    #[test]
    fn press_after_failed_capture_captures_again() {
        Desktop::run(|desktop| {
            desktop.press(ScreenMode::Area, "failed");
            desktop.fail_capture(0);
            desktop.press(ScreenMode::Area, "retried");
            desktop.finish_capture(1, 2);

            assert_eq!(desktop.frame(0).activation, activation("retried"));
        });
    }
}
