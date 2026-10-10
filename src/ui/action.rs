use gtk::prelude::*;

use crate::application::desktop::ShortcutSpec;

/// How an action was invoked.
#[derive(Clone, Debug, Default)]
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
    CaptureArea,
    TranslateSelection,
    CopyTranslation,
    ReplaceSelection,
    Settings,
    Shortcuts,
    Quit,
}

impl Action {
    /// Actions that can be assigned a global shortcut.
    pub const WITH_SHORTCUT: [Action; 4] =
        [Action::CaptureArea, Action::TranslateSelection, Action::CopyTranslation, Action::ReplaceSelection];
    /// Tray icon menu items before the toggles.
    pub const IN_MENU: [Action; 6] = [
        Action::CaptureArea,
        Action::TranslateSelection,
        Action::CopyTranslation,
        Action::ReplaceSelection,
        Action::Settings,
        Action::Shortcuts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Action::CaptureArea => "Перевести область экрана",
            Action::TranslateSelection => "Перевести выделенный текст",
            Action::CopyTranslation => "Скопировать перевод выделенного",
            Action::ReplaceSelection => "Заменить выделенный текст переводом",
            Action::Settings => "Настройки…",
            Action::Shortcuts => "Горячие клавиши…",
            Action::Quit => "Выйти",
        }
    }

    pub fn shortcut(self) -> Option<ShortcutSpec> {
        let (id, preferred_trigger) = match self {
            Action::CaptureArea => ("capture", "CTRL+ALT+R"),
            Action::TranslateSelection => ("translate-selection", "CTRL+ALT+W"),
            Action::CopyTranslation => ("copy-translation", "CTRL+ALT+C"),
            Action::ReplaceSelection => ("replace-selection", "CTRL+ALT+V"),
            Action::Settings | Action::Shortcuts | Action::Quit => return None,
        };
        Some(ShortcutSpec { id, description: self.title(), preferred_trigger })
    }

    pub fn by_shortcut_id(id: &str) -> Option<Self> {
        Self::WITH_SHORTCUT.into_iter().find(|action| action.shortcut().is_some_and(|spec| spec.id == id))
    }
}
