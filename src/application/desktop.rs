use anyhow::Result;
use image::RgbaImage;

/// Глобальное сочетание, которое приложение просит у рабочего стола.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub id: &'static str,
    pub description: &'static str,
    /// Предложение в формате портала, например `CTRL+ALT+R`; окончательно выбирает пользователь.
    pub preferred_trigger: &'static str,
}

/// Нажатие глобального сочетания.
pub struct ShortcutPress {
    pub id: String,
    /// Токен, с которым рабочий стол даст фокус окну, открытому в ответ на нажатие.
    pub activation_token: Option<String>,
}

pub trait GlobalShortcuts: Send + Sync {
    /// Привязывает сочетания и передаёт нажатия в `on_press`, пока работает приложение.
    fn listen(&self, specs: &[ShortcutSpec], on_press: Box<dyn Fn(ShortcutPress) + Send>) -> Result<()>;
    /// Назначенное сочетание в формате GTK, например `<Control><Alt>r`.
    fn accelerator(&self, spec: &ShortcutSpec) -> Option<String>;
    fn assign(&self, spec: &ShortcutSpec, accelerator: &str) -> Result<()>;
    fn clear(&self, spec: &ShortcutSpec) -> Result<()>;
}

pub trait ScreenCapture: Send + Sync {
    /// Снимок всех мониторов.
    fn capture(&self) -> Result<RgbaImage>;
}

/// Запуск приложения при входе в систему.
pub trait LoginAutostart: Send + Sync {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> Result<()>;
}
