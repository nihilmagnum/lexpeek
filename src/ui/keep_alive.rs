use std::rc::Rc;

use gtk::prelude::*;

/// Keeps `owner` alive as long as `window` exists: the handlers of the window hold their owner only weakly,
/// so that the owner and the window do not keep each other alive forever.
pub fn until_destroyed<T: 'static>(window: &impl IsA<gtk::Widget>, owner: Rc<T>) {
    window.connect_destroy(move |_| {
        let _ = &owner;
    });
}
