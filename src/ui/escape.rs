use gtk::gdk;
use gtk::prelude::*;

/// Escape closes the window.
pub fn closes_window(window: &gtk::Window) {
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.add_shortcut(gtk::Shortcut::new(
        Some(gtk::KeyvalTrigger::new(gdk::Key::Escape, gdk::ModifierType::empty())),
        Some(gtk::NamedAction::new("window.close")),
    ));
    window.add_controller(shortcuts);
}
