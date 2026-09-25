use std::collections::HashMap;

use anyhow::{Context, Result};
use gtk::prelude::*;
use gtk::{gio, glib};

/// Доступ приложения к xdg-desktop-portal.
pub struct PortalAccess;

impl PortalAccess {
    const PERMISSION_STORE: &str = "org.freedesktop.impl.portal.PermissionStore";
    const PERMISSION_STORE_PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";

    /// Портал запоминает приложение при первом обращении, поэтому регистрация идёт раньше
    /// снимков экрана и горячих клавиш.
    pub fn register(app_id: &str) -> Result<()> {
        glib::MainContext::default().block_on(async { ashpd::register_host_app(app_id.try_into()?).await })?;
        Ok(())
    }

    /// Фоновому приложению GNOME не показывает диалог разрешения снимков экрана, поэтому
    /// разрешение записывается заранее, если его ещё нет.
    pub fn allow_screenshots(app_id: &str) -> Result<()> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .context("session bus is unavailable")?;
        let permissions = Self::call(&bus, "Lookup", ("screenshot", "screenshot").to_variant())?;
        let granted = permissions
            .try_child_value(0)
            .and_then(|table| table.get::<HashMap<String, Vec<String>>>())
            .and_then(|table| table.get(app_id).cloned())
            .is_some_and(|values| values.iter().any(|value| value == "yes"));
        if !granted {
            let arguments = ("screenshot", true, "screenshot", app_id, vec!["yes"]).to_variant();
            Self::call(&bus, "SetPermission", arguments)?;
        }
        Ok(())
    }

    fn call(bus: &gio::DBusConnection, method: &str, arguments: glib::Variant) -> Result<glib::Variant> {
        bus.call_sync(
            Some(Self::PERMISSION_STORE),
            Self::PERMISSION_STORE_PATH,
            Self::PERMISSION_STORE,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            gio::Cancellable::NONE,
        )
        .with_context(|| format!("permission store {method} failed"))
    }
}
