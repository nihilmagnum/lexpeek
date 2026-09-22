use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::application::languages::{LanguageStatus, LanguagesService};
use crate::domain::language::Language;

/// Окно выбора пары языков и загрузки моделей распознавания.
pub struct LanguagesWindow {
    window: gtk::ApplicationWindow,
    body: gtk::Box,
    languages: Arc<LanguagesService>,
}

impl LanguagesWindow {
    pub fn show(app: &gtk::Application, languages: Arc<LanguagesService>) {
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(24)
            .margin_end(24)
            .build();
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Языки Lexpeek")
            .default_width(460)
            .default_height(640)
            .child(&gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&body).build())
            .build();

        let languages_window = Rc::new(Self { window: window.clone(), body, languages });
        languages_window.refresh();
        // Обработчики держат окно слабой ссылкой, живёт оно столько же, сколько окно GTK.
        window.connect_destroy(move |_| {
            let _ = &languages_window;
        });
        window.present();
    }

    fn refresh(self: &Rc<Self>) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        let status = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build();
        let overview = match self.languages.overview() {
            Ok(overview) => overview,
            Err(error) => {
                status.set_label(&format!("Не удалось прочитать настройки: {error:#}"));
                self.body.append(&status);
                return;
            }
        };

        self.body.append(&Self::heading("Перевод"));
        let installed: Vec<Language> =
            overview.languages.iter().filter(|status| status.installed).map(|status| status.language.clone()).collect();
        let native = Self::language_choice(&installed, &overview.pair.native);
        let foreign = Self::language_choice(&installed, &overview.pair.foreign);
        self.body.append(&Self::row("Мой язык", &native));
        self.body.append(&Self::row("Второй язык", &foreign));
        self.body.append(&Self::hint("Текст на своём языке переводится на второй, на любом другом на свой."));
        for choice in [&native, &foreign] {
            let installed = installed.clone();
            choice.connect_selected_notify(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[weak]
                native,
                #[weak]
                foreign,
                #[weak]
                status,
                move |_| window.choose(&installed, &native, &foreign, &status)
            ));
        }
        self.body.append(&status);

        self.body.append(&Self::heading("Распознавание"));
        for language_status in &overview.languages {
            self.body.append(&self.model_row(language_status, &status));
        }
        let add = gtk::Button::builder().label("Добавить язык из файла…").halign(gtk::Align::Start).margin_top(8).build();
        add.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            status,
            move |_| window.pick_model_file(&status)
        ));
        self.body.append(&add);
    }

    fn choose(
        self: &Rc<Self>,
        installed: &[Language],
        native: &gtk::DropDown,
        foreign: &gtk::DropDown,
        status: &gtk::Label,
    ) {
        let (Some(native), Some(foreign)) =
            (installed.get(native.selected() as usize), installed.get(foreign.selected() as usize))
        else {
            return;
        };
        if native == foreign {
            status.set_label("Языки должны различаться");
            return;
        }
        if let Err(error) = self.languages.choose(&native.code, &foreign.code) {
            status.set_label(&format!("Не удалось сохранить: {error:#}"));
            return;
        }
        // Окно перестраивается после выхода из обработчика, чтобы не удалять список, который его вызвал.
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.refresh()
        ));
    }

    fn model_row(self: &Rc<Self>, language_status: &LanguageStatus, status: &gtk::Label) -> gtk::Box {
        let language = language_status.language.clone();
        let name = language.name.clone();
        if language_status.built_in && !language_status.installed {
            let download = gtk::Button::with_label("Загрузить");
            download.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[weak]
                status,
                move |button| window.download(button, language.clone(), &status)
            ));
            return Self::row(&name, &download);
        }
        let remove = gtk::Button::with_label("Удалить");
        if language_status.in_pair {
            remove.set_sensitive(false);
            remove.set_tooltip_text(Some("Язык выбран для перевода"));
        }
        remove.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            status,
            move |_| window.remove(&language, &status)
        ));
        Self::row(&name, &remove)
    }

    fn download(self: &Rc<Self>, button: &gtk::Button, language: Language, status: &gtk::Label) {
        button.set_sensitive(false);
        button.set_label("Загрузка…");
        let languages = Arc::clone(&self.languages);
        let name = language.name.clone();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            button,
            #[weak]
            status,
            async move {
                match gio::spawn_blocking(move || languages.download(&language)).await {
                    Ok(Ok(())) => window.refresh(),
                    Ok(Err(error)) => {
                        status.set_label(&format!("Не удалось загрузить {name}: {error:#}"));
                        button.set_label("Загрузить");
                        button.set_sensitive(true);
                    }
                    Err(_) => status.set_label("Загрузка аварийно завершилась"),
                }
            }
        ));
    }

    fn remove(self: &Rc<Self>, language: &Language, status: &gtk::Label) {
        match self.languages.remove(language) {
            Ok(()) => self.refresh(),
            Err(error) => status.set_label(&format!("Не удалось удалить {}: {error:#}", language.name)),
        }
    }

    fn pick_model_file(self: &Rc<Self>, status: &gtk::Label) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Модели Tesseract"));
        filter.add_pattern("*.traineddata");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog =
            gtk::FileDialog::builder().title("Модель распознавания языка").filters(&filters).modal(true).build();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            status,
            async move {
                let chosen = dialog.open_future(Some(&window.window)).await.ok().and_then(|file| file.path());
                if let Some(file) = chosen {
                    window.describe_language(file, &status);
                }
            }
        ));
    }

    /// Спрашивает название и код языка для выбранной модели.
    fn describe_language(self: &Rc<Self>, file: PathBuf, status: &gtk::Label) {
        let stem = file.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let name = gtk::Entry::builder().placeholder_text("Грузинский").hexpand(true).build();
        let code = gtk::Entry::builder().text(stem).hexpand(true).build();
        let error = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build();
        let add =
            gtk::Button::builder().label("Добавить").halign(gtk::Align::End).css_classes(["suggested-action"]).build();

        let form = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(24)
            .margin_end(24)
            .build();
        form.append(&Self::row("Название", &name));
        form.append(&Self::row("Код языка", &code));
        form.append(&Self::hint(
            "Двухбуквенный код ISO 639-1, по нему переводчик понимает язык: ka грузинский, hy армянский, he иврит.",
        ));
        form.append(&error);
        form.append(&add);
        let dialog = gtk::Window::builder()
            .title("Новый язык")
            .transient_for(&self.window)
            .modal(true)
            .default_width(420)
            .child(&form)
            .build();

        add.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            dialog,
            #[weak]
            status,
            #[weak]
            name,
            #[weak]
            code,
            #[weak]
            error,
            move |_| match window.languages.add_from_file(&file, &code.text(), &name.text()) {
                Ok(()) => {
                    dialog.close();
                    status.set_label("");
                    window.refresh();
                }
                Err(message) => error.set_label(&format!("Не удалось добавить: {message:#}")),
            }
        ));
        dialog.present();
    }

    fn language_choice(installed: &[Language], selected: &Language) -> gtk::DropDown {
        let names: Vec<&str> = installed.iter().map(|language| &*language.name).collect();
        let choice = gtk::DropDown::from_strings(&names);
        if let Some(position) = installed.iter().position(|language| language == selected) {
            choice.set_selected(position as u32);
        }
        choice
    }

    fn row(title: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.append(&gtk::Label::builder().label(title).xalign(0.0).hexpand(true).build());
        row.append(control);
        row
    }

    fn heading(title: &str) -> gtk::Label {
        gtk::Label::builder().label(title).xalign(0.0).css_classes(["heading"]).margin_top(8).build()
    }

    fn hint(text: &str) -> gtk::Label {
        gtk::Label::builder().label(text).xalign(0.0).wrap(true).css_classes(["dim-label"]).build()
    }
}
