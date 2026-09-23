use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::application::desktop::{GlobalShortcuts, ShortcutSpec};
use crate::ui::action::Action;
use crate::ui::shortcut_editor::ShortcutEditor;

/// Окно со списком действий и их горячих клавиш.
pub struct ShortcutsWindow {
    app: gtk::Application,
    body: gtk::Box,
    shortcuts: Arc<dyn GlobalShortcuts>,
}

impl ShortcutsWindow {
    pub fn show(app: &gtk::Application, shortcuts: Arc<dyn GlobalShortcuts>) {
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(24)
            .margin_end(24)
            .build();
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Горячие клавиши Lexpeek")
            .default_width(520)
            .child(&body)
            .build();

        let shortcuts_window = Rc::new(Self { app: app.clone(), body, shortcuts });
        shortcuts_window.refresh();
        // Обработчики держат окно слабой ссылкой, живёт оно столько же, сколько окно GTK.
        window.connect_destroy(move |_| {
            let _ = &shortcuts_window;
        });
        window.present();
    }

    fn refresh(self: &Rc<Self>) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        let status = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build();
        for action in Action::WITH_SHORTCUT {
            if let Some(spec) = action.shortcut() {
                let accelerator = self.shortcuts.accelerator(&spec);
                let current = gtk::Label::builder()
                    .label(accelerator.as_deref().map(Self::label).unwrap_or_else(|| "не задано".into()))
                    .css_classes(["dim-label"])
                    .build();
                let change = gtk::Button::with_label("Изменить");
                change.connect_clicked(glib::clone!(
                    #[weak(rename_to = window)]
                    self,
                    move |_| {
                        let on_saved: Rc<dyn Fn()> = Rc::new(glib::clone!(
                            #[weak]
                            window,
                            move || window.refresh()
                        ));
                        ShortcutEditor::show(&window.app, Arc::clone(&window.shortcuts), spec, on_saved);
                    }
                ));
                let clear = gtk::Button::builder().label("Очистить").sensitive(accelerator.is_some()).build();
                clear.connect_clicked(glib::clone!(
                    #[weak(rename_to = window)]
                    self,
                    #[weak]
                    status,
                    move |_| window.clear(spec, &status)
                ));
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.append(&gtk::Label::builder().label(action.title()).xalign(0.0).hexpand(true).build());
                row.append(&current);
                row.append(&change);
                row.append(&clear);
                self.body.append(&row);
            }
        }
        self.body.append(&status);
    }

    fn clear(self: &Rc<Self>, spec: ShortcutSpec, status: &gtk::Label) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            status,
            async move {
                let shortcuts = Arc::clone(&window.shortcuts);
                match gio::spawn_blocking(move || shortcuts.clear(&spec)).await {
                    Ok(Ok(())) => window.refresh(),
                    Ok(Err(error)) => status.set_label(&format!("Не удалось очистить: {error:#}")),
                    Err(_) => status.set_label("Очистка аварийно завершилась"),
                }
            }
        ));
    }

    fn label(accelerator: &str) -> String {
        gtk::accelerator_parse(accelerator)
            .map(|(key, modifiers)| gtk::accelerator_get_label(key, modifiers).to_string())
            .unwrap_or_else(|| accelerator.to_owned())
    }
}
