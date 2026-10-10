use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use gtk::glib;
use rust_i18n::t;

use crate::application::interface::InterfaceLanguageSettings;
use crate::domain::language::LanguageCode;

/// Language the interface is translated into.
pub struct InterfaceTranslation {
    pub code: LanguageCode,
    /// Name of the language in itself, so that its speakers recognize it.
    pub endonym: String,
}

/// Interface language: its choice and switching.
pub struct InterfaceLanguage {
    settings: Arc<dyn InterfaceLanguageSettings>,
    listeners: RefCell<Vec<Box<dyn Fn() -> glib::ControlFlow>>>,
}

impl InterfaceLanguage {
    /// Interface language when neither the chosen language nor the system language is translated.
    const FALLBACK: &str = "en";

    pub fn new(settings: Arc<dyn InterfaceLanguageSettings>) -> Self {
        Self { settings, listeners: RefCell::default() }
    }

    /// Language the interface is currently shown in.
    pub fn current() -> LanguageCode {
        LanguageCode::parse(&rust_i18n::locale()).expect("interface translations are named by language codes")
    }

    pub fn translations() -> Vec<InterfaceTranslation> {
        rust_i18n::available_locales!()
            .into_iter()
            .filter_map(|locale| {
                let endonym = t!("endonym", locale = &locale).into_owned();
                Some(InterfaceTranslation { code: LanguageCode::parse(&locale).ok()?, endonym })
            })
            .collect()
    }

    /// `None` if the interface follows the system language.
    pub fn chosen(&self) -> Result<Option<LanguageCode>> {
        self.settings.interface_language()
    }

    /// Saves the choice and switches the interface to it immediately.
    pub fn choose(&self, code: Option<&LanguageCode>) -> Result<()> {
        self.settings.set_interface_language(code)?;
        Self::switch(code.cloned());
        self.listeners.borrow_mut().retain(|listener| listener().is_continue());
        Ok(())
    }

    /// Calls `follow` with `follower` after every switch while `follower` is alive.
    pub fn connect_switched<T: 'static>(&self, follower: &Rc<T>, follow: impl Fn(&Rc<T>) + 'static) {
        let follower = Rc::downgrade(follower);
        self.listeners.borrow_mut().push(Box::new(move || match follower.upgrade() {
            Some(follower) => {
                follow(&follower);
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        }));
    }

    /// Switches the interface to the saved choice, or to the system language if there is none or it cannot be read.
    pub fn apply(&self) {
        let chosen = self.chosen().unwrap_or_else(|error| {
            eprintln!("lexpeek: {error:#}");
            None
        });
        Self::switch(chosen);
    }

    fn switch(chosen: Option<LanguageCode>) {
        let system: Vec<String> = glib::language_names().iter().map(ToString::to_string).collect();
        rust_i18n::set_locale(&Self::resolve(chosen, &system));
    }

    /// The chosen language if the interface is translated into it, otherwise the first translated system language.
    /// System languages come as locale names like `ru_RU.UTF-8`.
    fn resolve(chosen: Option<LanguageCode>, system: &[String]) -> String {
        let available = rust_i18n::available_locales!();
        let system = system.iter().map(|locale| locale.split(['_', '.', '@']).next().unwrap_or(locale).to_owned());
        chosen
            .map(|code| code.to_string())
            .into_iter()
            .chain(system)
            .find(|code| available.iter().any(|locale| locale == code))
            .unwrap_or_else(|| Self::FALLBACK.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn follows_system_language_until_one_is_chosen() {
        let russian = ["ru_RU.UTF-8", "ru", "C"].map(String::from);
        let german = ["de_DE.UTF-8", "C"].map(String::from);

        assert_eq!(InterfaceLanguage::resolve(None, &russian), "ru");
        assert_eq!(InterfaceLanguage::resolve(None, &german), "en");
        assert_eq!(InterfaceLanguage::resolve(LanguageCode::parse("en").ok(), &russian), "en");
    }

    #[test]
    fn every_translation_has_all_texts() {
        let backend = crate::_rust_i18n_backend();
        let keys = |locale: &str| -> BTreeSet<String> {
            let messages = backend.messages_for_locale(locale).unwrap_or_default();
            messages.into_iter().map(|(key, _)| key.into_owned()).collect()
        };
        for translation in InterfaceLanguage::translations() {
            assert_eq!(keys(translation.code.as_str()), keys(InterfaceLanguage::FALLBACK), "{}", translation.code);
        }
    }
}
