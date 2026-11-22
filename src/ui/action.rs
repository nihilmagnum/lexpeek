use gtk::prelude::*;
use rust_i18n::t;

use crate::application::desktop::ShortcutSpec;

/// How an action was invoked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activation {
    token: Option<String>,
}

impl Activation {
    pub fn with_token(token: Option<String>) -> Self {
        Self { token }
    }

    /// GNOME gives focus to a window of a background application only with an activation token.
    pub fn present(&self, window: &impl IsA<gtk::Window>) {
        if let Some(token) = &self.token {
            window.set_startup_id(token);
        }
        window.present();
    }
}

/// User action: invoked by a shortcut or from the tray icon menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    TranslateArea,
    TranslateScreen,
    TranslateSelection,
    CopyTranslation,
    ReplaceSelection,
    Settings,
    Shortcuts,
    Quit,
}

impl Action {
    /// Actions that can be assigned a global shortcut.
    pub const WITH_SHORTCUT: [Action; 5] = [
        Action::TranslateArea,
        Action::TranslateScreen,
        Action::TranslateSelection,
        Action::CopyTranslation,
        Action::ReplaceSelection,
    ];
    /// Tray icon menu items before the toggles.
    pub const IN_MENU: [Action; 7] = [
        Action::TranslateArea,
        Action::TranslateScreen,
        Action::TranslateSelection,
        Action::CopyTranslation,
        Action::ReplaceSelection,
        Action::Settings,
        Action::Shortcuts,
    ];

    pub fn title(self) -> String {
        let title = match self {
            Action::TranslateArea => t!("action.translate_area"),
            Action::TranslateScreen => t!("action.translate_screen"),
            Action::TranslateSelection => t!("action.translate_selection"),
            Action::CopyTranslation => t!("action.copy_translation"),
            Action::ReplaceSelection => t!("action.replace_selection"),
            Action::Settings => t!("action.settings"),
            Action::Shortcuts => t!("action.shortcuts"),
            Action::Quit => t!("action.quit"),
        };
        title.into_owned()
    }

    pub fn shortcut(self) -> Option<ShortcutSpec> {
        // The desktop keeps assigned shortcuts by id, so a changed id loses the assignment.
        let (id, preferred_trigger) = match self {
            Action::TranslateArea => ("capture", "CTRL+ALT+R"),
            Action::TranslateScreen => ("translate-screen", "CTRL+ALT+S"),
            Action::TranslateSelection => ("translate-selection", "CTRL+ALT+W"),
            Action::CopyTranslation => ("copy-translation", "CTRL+ALT+C"),
            Action::ReplaceSelection => ("replace-selection", "CTRL+ALT+V"),
            Action::Settings | Action::Shortcuts | Action::Quit => return None,
        };
        Some(ShortcutSpec { id, description: self.title(), preferred_trigger })
    }

    pub fn shortcut_specs() -> Vec<ShortcutSpec> {
        Self::WITH_SHORTCUT.into_iter().filter_map(Action::shortcut).collect()
    }

    pub fn by_shortcut_id(id: &str) -> Option<Self> {
        Self::WITH_SHORTCUT.into_iter().find(|action| action.shortcut().is_some_and(|spec| spec.id == id))
    }
}
