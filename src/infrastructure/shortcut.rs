use std::collections::HashMap;

use anyhow::{Context, Result};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use gtk::{gio, glib};
use gtk::prelude::*;

/// Глобальная горячая клавиша перевода, назначенная через портал GNOME.
pub struct CaptureShortcut;

impl CaptureShortcut {
    const ID: &str = "capture";
    const DESCRIPTION: &str = "Перевести область экрана";
    const PREFERRED_TRIGGER: &str = "CTRL+ALT+R";
    const REGISTRY_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts";
    const APPLICATION_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts.application";

    /// Привязывает сочетание (в первый раз GNOME спрашивает его в своём диалоге) и вызывает
    /// `on_press` при каждом нажатии.
    pub async fn listen(on_press: impl Fn()) -> ashpd::Result<()> {
        let portal = GlobalShortcuts::new().await?;
        let session = portal.create_session(Default::default()).await?;
        let mut activations = portal.receive_activated().await?;
        let shortcut = NewShortcut::new(Self::ID, Self::DESCRIPTION).preferred_trigger(Self::PREFERRED_TRIGGER);
        portal.bind_shortcuts(&session, &[shortcut], None, Default::default()).await?.response()?;
        while let Some(activation) = activations.next().await {
            if activation.shortcut_id() == Self::ID {
                on_press();
            }
        }
        Ok(())
    }

    /// Меняет сочетание так же, как Настройки GNOME: сохраняет его в настройках
    /// gnome-settings-daemon и передаёт порталу, чтобы оно заработало сразу.
    /// Портал версии 1 сам сменить сочетание не даёт.
    pub async fn rebind(app_id: &str, accelerator: &str) -> Result<()> {
        let options = HashMap::from([
            ("shortcuts".to_owned(), vec![accelerator.to_owned()].to_variant()),
            ("description".to_owned(), Self::DESCRIPTION.to_variant()),
        ]);
        let shortcuts = vec![(Self::ID.to_owned(), options)].to_variant();

        let registry = gio::Settings::new(Self::REGISTRY_SCHEMA);
        let mut applications: Vec<String> = registry.strv("applications").iter().map(ToString::to_string).collect();
        if !applications.iter().any(|id| id == app_id) {
            applications.push(app_id.to_owned());
            registry.set_strv("applications", applications.iter().map(String::as_str).collect::<Vec<_>>())?;
        }
        let path = format!("/org/gnome/settings-daemon/global-shortcuts/{app_id}/");
        gio::Settings::with_path(Self::APPLICATION_SCHEMA, &path).set_value("shortcuts", &shortcuts)?;
        gio::Settings::sync();

        let bus = gio::bus_get_future(gio::BusType::Session).await.context("session bus is unavailable")?;
        bus.call_future(
            Some("org.freedesktop.impl.portal.desktop.gnome"),
            "/org/gnome/globalshortcuts",
            "org.gnome.GlobalShortcutsRebind",
            "RebindShortcuts",
            Some(&glib::Variant::tuple_from_iter([app_id.to_variant(), shortcuts])),
            None,
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
        .context("GNOME did not accept the new shortcut")?;
        Ok(())
    }
}
