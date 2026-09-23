use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::application::desktop::{GlobalShortcuts, ShortcutSpec};

/// Нажатие в окне редактора.
struct PressedKeys {
    key: gdk::Key,
    keycode: u32,
    state: gdk::ModifierType,
}

/// Окно, в котором нажимают новое сочетание для действия.
pub struct ShortcutEditor;

impl ShortcutEditor {
    const LAYOUT_GROUPS: i32 = 4;

    /// `on_saved` вызывается, когда GNOME принял новое сочетание.
    pub fn show(
        app: &gtk::Application,
        shortcuts: Arc<dyn GlobalShortcuts>,
        spec: ShortcutSpec,
        on_saved: Rc<dyn Fn()>,
    ) {
        let status = gtk::Label::builder().label("Esc — отмена").css_classes(["dim-label"]).wrap(true).build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(28)
            .margin_bottom(28)
            .margin_start(32)
            .margin_end(32)
            .build();
        content.append(&gtk::Label::builder().label(spec.description).css_classes(["title-3"]).build());
        content.append(&gtk::Label::builder().label("Нажмите новое сочетание клавиш").build());
        content.append(&status);

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Горячая клавиша Lexpeek")
            .resizable(false)
            .default_width(380)
            .child(&content)
            .build();

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            window,
            #[weak]
            status,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, key, keycode, state| {
                let pressed = PressedKeys { key, keycode, state };
                let shortcuts = Arc::clone(&shortcuts);
                Self::key_pressed(controller, &window, &status, shortcuts, spec, Rc::clone(&on_saved), pressed)
            }
        ));
        window.add_controller(keys);
        window.present();
    }

    fn key_pressed(
        controller: &gtk::EventControllerKey,
        window: &gtk::ApplicationWindow,
        status: &gtk::Label,
        shortcuts: Arc<dyn GlobalShortcuts>,
        spec: ShortcutSpec,
        on_saved: Rc<dyn Fn()>,
        pressed: PressedKeys,
    ) -> glib::Propagation {
        let PressedKeys { key, keycode, state } = pressed;
        let modifiers = state & gtk::accelerator_get_default_mod_mask();
        if key == gdk::Key::Escape && modifiers.is_empty() {
            window.close();
            return glib::Propagation::Stop;
        }
        let event = controller.current_event().and_then(|event| event.downcast::<gdk::KeyEvent>().ok());
        let is_modifier = event.is_some_and(|event| event.is_modifier());
        if is_modifier {
            return glib::Propagation::Proceed;
        }
        if (modifiers - gdk::ModifierType::SHIFT_MASK).is_empty() {
            status.set_label("Нужен Ctrl, Alt или Super");
            return glib::Propagation::Stop;
        }

        let latin = Self::latin_key(window, keycode, key).to_lower();
        let accelerator = gtk::accelerator_name(latin, modifiers);
        status.set_label(&format!("Сохраняю {}…", gtk::accelerator_get_label(latin, modifiers)));
        glib::spawn_future_local(glib::clone!(
            #[weak]
            window,
            #[weak]
            status,
            async move {
                match gio::spawn_blocking(move || shortcuts.assign(&spec, &accelerator)).await {
                    Ok(Ok(())) => {
                        window.close();
                        on_saved();
                    }
                    Ok(Err(error)) => status.set_label(&format!("Не удалось сохранить: {error:#}")),
                    Err(_) => status.set_label("Сохранение аварийно завершилось"),
                }
            }
        ));
        glib::Propagation::Stop
    }

    /// Клавиша в латинской раскладке: сочетание, записанное в русской, срабатывало бы только в ней.
    fn latin_key(window: &gtk::ApplicationWindow, keycode: u32, pressed: gdk::Key) -> gdk::Key {
        let display = WidgetExt::display(window);
        (0..Self::LAYOUT_GROUPS)
            .filter_map(|group| display.translate_key(keycode, gdk::ModifierType::empty(), group))
            .map(|(key, ..)| key)
            .find(|key| key.to_unicode().is_some_and(|symbol| symbol.is_ascii_graphic()))
            .unwrap_or(pressed)
    }
}
