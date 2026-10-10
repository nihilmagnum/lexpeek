use anyhow::Result;
use image::RgbaImage;

/// Global shortcut the application requests from the desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub id: &'static str,
    pub description: String,
    /// Suggestion in portal format, for example `CTRL+ALT+R`; the user makes the final choice.
    pub preferred_trigger: &'static str,
}

/// Press of a global shortcut.
pub struct ShortcutPress {
    pub id: String,
    /// Token with which the desktop gives focus to a window opened in response to the press.
    pub activation_token: Option<String>,
}

pub trait GlobalShortcuts: Send + Sync {
    /// Binds the shortcuts and passes presses to `on_press` while the application runs.
    fn listen(&self, specs: &[ShortcutSpec], on_press: Box<dyn Fn(ShortcutPress) + Send>) -> Result<()>;
    /// Assigned shortcut in GTK format, for example `<Control><Alt>r`.
    fn accelerator(&self, spec: &ShortcutSpec) -> Option<String>;
    fn assign(&self, spec: &ShortcutSpec, accelerator: &str) -> Result<()>;
    fn clear(&self, spec: &ShortcutSpec) -> Result<()>;
    /// Updates the descriptions the desktop shows for the assigned shortcuts.
    fn describe(&self, specs: &[ShortcutSpec]) -> Result<()>;
}

pub trait ScreenCapture: Send + Sync {
    /// Screenshot of all monitors.
    fn capture(&self) -> Result<RgbaImage>;
}

/// Launching the application at login.
pub trait LoginAutostart: Send + Sync {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> Result<()>;
}
