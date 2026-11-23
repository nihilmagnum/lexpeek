use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{gio, glib};
use rust_i18n::t;

use crate::application::languages::{LanguageStatus, LanguagesError, LanguagesService};
use crate::application::translators::Translators;
use crate::domain::language::{Language, LanguageCode, LanguageError, LanguagePair};
use crate::ui::background;
use crate::ui::interface_language::InterfaceLanguage;
use crate::ui::keep_alive;

/// Settings window: interface language, translator, language pair and recognition models.
pub struct SettingsWindow {
    window: gtk::ApplicationWindow,
    body: gtk::Box,
    languages: Arc<LanguagesService>,
    translators: Arc<Translators>,
    interface_language: Rc<InterfaceLanguage>,
}

impl SettingsWindow {
    pub fn show(
        app: &gtk::Application,
        languages: Arc<LanguagesService>,
        translators: Arc<Translators>,
        interface_language: Rc<InterfaceLanguage>,
    ) {
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
            .default_width(460)
            .default_height(640)
            .child(&gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&body).build())
            .build();

        let settings_window =
            Rc::new(Self { window: window.clone(), body, languages, translators, interface_language });
        settings_window.refresh();
        settings_window.interface_language.connect_switched(&settings_window, |window| window.refresh_later());
        keep_alive::until_destroyed(&window, settings_window);
        window.present();
    }

    fn refresh(self: &Rc<Self>) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        self.window.set_title(Some(&t!("settings.title")));
        let status = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build();
        let overview = match self.languages.overview() {
            Ok(overview) => overview,
            Err(error) => {
                status.set_label(&t!("settings.read_failed", error = format!("{error:#}")));
                self.body.append(&status);
                return;
            }
        };

        self.body.append(&Self::heading(&t!("settings.interface")));
        self.body.append(&self.interface_language_choice(&status));

        self.body.append(&Self::heading(&t!("settings.translation")));
        self.body.append(&self.translator_choice(&status));
        let installed: Vec<Language> =
            overview.languages.iter().filter(|status| status.installed).map(|status| status.language.clone()).collect();
        let native = Self::language_choice(&installed, overview.pair.native());
        let foreign = Self::language_choice(&installed, overview.pair.foreign());
        self.body.append(&Self::row(&t!("settings.native_language"), &native));
        self.body.append(&Self::row(&t!("settings.foreign_language"), &foreign));
        self.body.append(&Self::hint(&t!("settings.pair_hint")));
        let save_pair = |choice: &gtk::DropDown, change: fn(&LanguagePair, Language) -> LanguagePair| {
            let (installed, pair) = (installed.clone(), overview.pair.clone());
            let choose = glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                Ok(()),
                move |position: u32| match installed.get(position as usize) {
                    Some(language) => window.choose(&change(&pair, language.clone())),
                    None => Ok(()),
                }
            );
            Self::save_selection(choice, &status, choose);
        };
        save_pair(&native, LanguagePair::with_native);
        save_pair(&foreign, LanguagePair::with_foreign);
        self.body.append(&status);

        self.body.append(&Self::heading(&t!("settings.recognition")));
        for language_status in &overview.languages {
            self.body.append(&self.model_row(language_status, &status));
        }
        let add = gtk::Button::builder()
            .label(t!("settings.add_from_file"))
            .halign(gtk::Align::Start)
            .margin_top(8)
            .build();
        add.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            status,
            move |_| window.pick_model_file(&status)
        ));
        self.body.append(&add);
    }

    /// The first item of the list keeps the interface in the system language.
    fn interface_language_choice(&self, status: &gtk::Label) -> gtk::Box {
        let chosen = self.interface_language.chosen().unwrap_or_else(|error| {
            status.set_label(&t!("settings.read_failed", error = format!("{error:#}")));
            None
        });
        let translations = InterfaceLanguage::translations();
        let endonyms = translations.iter().map(|translation| translation.endonym.clone());
        let names: Vec<String> = [t!("settings.system_language").into_owned()].into_iter().chain(endonyms).collect();
        let choice = Self::drop_down(&names);
        let position = chosen.and_then(|code| translations.iter().position(|translation| translation.code == code));
        choice.set_selected(position.map_or(0, |position| position as u32 + 1));
        let interface_language = Rc::clone(&self.interface_language);
        Self::save_selection(&choice, status, move |position| {
            let selected = (position as usize).checked_sub(1).and_then(|index| translations.get(index));
            interface_language.choose(selected.map(|translation| &translation.code))
        });
        Self::row(&t!("settings.interface_language"), &choice)
    }

    fn translator_choice(&self, status: &gtk::Label) -> gtk::Box {
        let ids = self.translators.ids();
        let names: Vec<String> = ids.iter().map(|id| t!(format!("translators.{id}")).into_owned()).collect();
        let choice = Self::drop_down(&names);
        let selected = self.translators.selected().ok().map(|translator| translator.id());
        if let Some(position) = ids.iter().position(|id| Some(*id) == selected) {
            choice.set_selected(position as u32);
        }
        let translators = Arc::clone(&self.translators);
        Self::save_selection(&choice, status, move |position| match ids.get(position as usize) {
            Some(id) => translators.select(id),
            None => Ok(()),
        });
        Self::row(&t!("settings.translator"), &choice)
    }

    fn choose(self: &Rc<Self>, pair: &LanguagePair) -> Result<(), LanguagesError> {
        self.languages.choose(pair.native().code(), pair.foreign().code())?;
        self.refresh_later();
        Ok(())
    }

    /// Saves every new selection of the list; one that could not be saved is taken back, so the list
    /// shows what is saved.
    fn save_selection<E: Explained>(
        choice: &gtk::DropDown,
        status: &gtk::Label,
        save: impl Fn(u32) -> Result<(), E> + 'static,
    ) {
        let saved = Cell::new(choice.selected());
        choice.connect_selected_notify(glib::clone!(
            #[weak]
            status,
            move |choice| {
                let selected = choice.selected();
                if selected == saved.get() {
                    return;
                }
                match save(selected) {
                    Ok(()) => saved.set(selected),
                    Err(error) => {
                        status.set_label(&t!("settings.save_failed", error = error.explanation()));
                        choice.set_selected(saved.get());
                    }
                }
            }
        ));
    }

    /// The window is rebuilt after the current handler returns, so as not to remove the list that called it.
    fn refresh_later(self: &Rc<Self>) {
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.refresh()
        ));
    }

    fn model_row(self: &Rc<Self>, language_status: &LanguageStatus, status: &gtk::Label) -> gtk::Box {
        let language = language_status.language.clone();
        let name = Self::language_name(&language);
        if language.is_built_in() && !language_status.installed {
            let download = gtk::Button::with_label(&t!("settings.download"));
            download.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[weak]
                status,
                move |button| window.download(button, language.clone(), &status)
            ));
            return Self::row(&name, &download);
        }
        let remove = gtk::Button::with_label(&t!("settings.remove"));
        if language_status.in_pair {
            remove.set_sensitive(false);
            remove.set_tooltip_text(Some(&t!("settings.chosen_for_translation")));
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
        button.set_label(&t!("settings.downloading"));
        let languages = Arc::clone(&self.languages);
        let name = Self::language_name(&language);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            button,
            #[weak]
            status,
            async move {
                match background::run(move || languages.download(&language)).await {
                    Ok(()) => window.refresh(),
                    Err(error) => {
                        let error = error.explanation();
                        status.set_label(&t!("settings.download_failed", language = name, error = error));
                        button.set_label(&t!("settings.download"));
                        button.set_sensitive(true);
                    }
                }
            }
        ));
    }

    fn remove(self: &Rc<Self>, language: &Language, status: &gtk::Label) {
        match self.languages.remove(language) {
            Ok(()) => self.refresh(),
            Err(error) => {
                let name = Self::language_name(language);
                status.set_label(&t!("settings.remove_failed", language = name, error = error.explanation()))
            }
        }
    }

    fn pick_model_file(self: &Rc<Self>, status: &gtk::Label) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&t!("settings.models_filter")));
        filter.add_pattern("*.traineddata");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog =
            gtk::FileDialog::builder().title(t!("settings.model_dialog")).filters(&filters).modal(true).build();
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

    /// Asks for the name and code of the language of the chosen model.
    fn describe_language(self: &Rc<Self>, file: PathBuf, status: &gtk::Label) {
        let stem = file.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let name = gtk::Entry::builder().placeholder_text(t!("settings.language_name_example")).hexpand(true).build();
        let code = gtk::Entry::builder().text(stem).hexpand(true).build();
        let error = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build();
        let add = gtk::Button::builder()
            .label(t!("settings.add"))
            .halign(gtk::Align::End)
            .css_classes(["suggested-action"])
            .build();

        let form = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(24)
            .margin_end(24)
            .build();
        form.append(&Self::row(&t!("settings.language_name"), &name));
        form.append(&Self::row(&t!("settings.language_code"), &code));
        form.append(&Self::hint(&t!("settings.language_code_hint")));
        form.append(&error);
        form.append(&add);
        let dialog = gtk::Window::builder()
            .title(t!("settings.new_language"))
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
            move |add| {
                let languages = Arc::clone(&window.languages);
                let (file, code, name) = (file.clone(), code.text().to_string(), name.text().to_string());
                add.set_sensitive(false);
                glib::spawn_future_local(glib::clone!(
                    #[weak]
                    window,
                    #[weak]
                    dialog,
                    #[weak]
                    status,
                    #[weak]
                    error,
                    #[weak]
                    add,
                    async move {
                        // A model may weigh tens of megabytes.
                        let added =
                            background::run(move || languages.add_from_file(&file, &LanguageCode::parse(&code)?, &name))
                                .await;
                        add.set_sensitive(true);
                        match added {
                            Ok(()) => {
                                dialog.close();
                                status.set_label("");
                                window.refresh();
                            }
                            Err(message) => {
                                error.set_label(&t!("settings.add_failed", error = message.explanation()))
                            }
                        }
                    }
                ));
            }
        ));
        dialog.present();
    }

    fn language_choice(installed: &[Language], selected: &Language) -> gtk::DropDown {
        let names: Vec<String> = installed.iter().map(Self::language_name).collect();
        let choice = Self::drop_down(&names);
        if let Some(position) = installed.iter().position(|language| language == selected) {
            choice.set_selected(position as u32);
        }
        choice
    }

    /// User-friendly explanation of an error; unexpected errors are shown as is.

    /// Built-in languages are named in the interface language, added ones the way the user named them.
    fn language_name(language: &Language) -> String {
        match language {
            Language::BuiltIn(language) => t!(format!("languages.{}", language.code())).into_owned(),
            Language::Custom(language) => language.name().to_owned(),
        }
    }

    fn drop_down(names: &[String]) -> gtk::DropDown {
        gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>())
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

/// Failure told to the user in the interface language.
trait Explained {
    fn explanation(&self) -> String;
}

impl Explained for anyhow::Error {
    fn explanation(&self) -> String {
        format!("{self:#}")
    }
}

impl Explained for LanguagesError {
    fn explanation(&self) -> String {
        let explanation = match self {
            LanguagesError::Language(LanguageError::InvalidCode(_)) => t!("errors.invalid_code"),
            LanguagesError::Language(LanguageError::EmptyName) => t!("errors.empty_name"),
            LanguagesError::Language(LanguageError::AlreadyInCatalog(code)) => {
                t!("errors.already_in_catalog", code = code)
            }
            LanguagesError::Language(LanguageError::Unknown(code)) => t!("errors.unknown_language", code = code),
            LanguagesError::Language(LanguageError::SameLanguagesInPair) => t!("errors.same_languages"),
            LanguagesError::Language(LanguageError::ChosenForTranslation(_)) => t!("errors.chosen_for_translation"),
            LanguagesError::ModelMissing(code) => t!("errors.model_missing", code = code),
            LanguagesError::NotDownloadable(_) => t!("errors.not_downloadable"),
            LanguagesError::Failed(error) => return error.explanation(),
        };
        explanation.into_owned()
    }
}
