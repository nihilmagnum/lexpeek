use anyhow::Result;

use crate::domain::language::LanguageCode;

/// Interface language chosen by the user.
pub trait InterfaceLanguageSettings: Send + Sync {
    /// `None` until the user chooses a language: the interface then follows the system language.
    fn interface_language(&self) -> Result<Option<LanguageCode>>;
    fn set_interface_language(&self, code: Option<&LanguageCode>) -> Result<()>;
}
