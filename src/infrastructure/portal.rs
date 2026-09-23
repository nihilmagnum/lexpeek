use anyhow::Result;
use gtk::glib;

/// Регистрация приложения в xdg-desktop-portal.
pub struct PortalRegistration;

impl PortalRegistration {
    /// Портал запоминает приложение при первом обращении, поэтому регистрация идёт раньше
    /// снимков экрана и горячих клавиш.
    pub fn register(app_id: &str) -> Result<()> {
        glib::MainContext::default().block_on(async { ashpd::register_host_app(app_id.try_into()?).await })?;
        Ok(())
    }
}
