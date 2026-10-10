use std::collections::HashMap;

use anyhow::{Context, Result};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts as ShortcutsPortal, NewShortcut};
use futures_util::StreamExt;
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::application::desktop::{GlobalShortcuts, ShortcutPress, ShortcutSpec};

/// Application shortcut as gnome-settings-daemon stores it: an element of the `a(sa{sv})` array.
struct StoredShortcut {
    id: String,
    accelerators: Vec<String>,
    description: String,
}

impl StoredShortcut {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let id = variant.try_child_value(0)?.get::<String>()?;
        let options = variant.try_child_value(1)?.get::<HashMap<String, glib::Variant>>()?;
        let accelerators = options.get("shortcuts").and_then(|value| value.get::<Vec<String>>()).unwrap_or_default();
        let description = options.get("description").and_then(|value| value.get::<String>()).unwrap_or_default();
        Some(Self { id, accelerators, description })
    }

    fn to_variant(&self) -> glib::Variant {
        let options = HashMap::from([
            ("shortcuts".to_owned(), self.accelerators.to_variant()),
            ("description".to_owned(), self.description.to_variant()),
        ]);
        glib::Variant::tuple_from_iter([self.id.to_variant(), options.to_variant()])
    }
}

/// Global shortcuts through the GNOME portal.
pub struct PortalShortcuts {
    app_id: &'static str,
}

impl PortalShortcuts {
    const REGISTRY_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts";
    const APPLICATION_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts.application";

    pub fn new(app_id: &'static str) -> Self {
        Self { app_id }
    }

    async fn bind_and_listen(specs: &[ShortcutSpec], on_press: &dyn Fn(ShortcutPress)) -> ashpd::Result<()> {
        let portal = ShortcutsPortal::new().await?;
        let session = portal.create_session(Default::default()).await?;
        let mut activations = portal.receive_activated().await?;
        let shortcuts: Vec<NewShortcut> = specs
            .iter()
            .map(|spec| NewShortcut::new(spec.id, &spec.description).preferred_trigger(spec.preferred_trigger))
            .collect();
        portal.bind_shortcuts(&session, &shortcuts, None, Default::default()).await?.response()?;
        while let Some(activation) = activations.next().await {
            let token = activation.options().get("activation_token").and_then(|token| <&str>::try_from(token).ok());
            let id = activation.shortcut_id().to_owned();
            on_press(ShortcutPress { id, activation_token: token.map(str::to_owned) });
        }
        Ok(())
    }

    fn bind(&self, spec: &ShortcutSpec, accelerators: Vec<String>) -> Result<()> {
        let mut stored = self.stored();
        stored.retain(|shortcut| shortcut.id != spec.id);
        stored.push(StoredShortcut { id: spec.id.to_owned(), accelerators, description: spec.description.clone() });
        self.store(&stored)
    }

    /// Changes shortcuts the same way GNOME Settings does: stores them in the settings of
    /// gnome-settings-daemon and passes them to the portal so that they work immediately.
    /// Version 1 of the portal provides no way to change a shortcut.
    fn store(&self, stored: &[StoredShortcut]) -> Result<()> {
        let shortcuts = glib::Variant::array_from_iter_with_type(
            glib::VariantTy::new("(sa{sv})").expect("valid variant type"),
            stored.iter().map(StoredShortcut::to_variant),
        );

        let registry = gio::Settings::new(Self::REGISTRY_SCHEMA);
        let mut applications: Vec<String> = registry.strv("applications").iter().map(ToString::to_string).collect();
        if !applications.iter().any(|id| id == self.app_id) {
            applications.push(self.app_id.to_owned());
            registry.set_strv("applications", applications.iter().map(String::as_str).collect::<Vec<_>>())?;
        }
        self.application_settings().set_value("shortcuts", &shortcuts)?;
        gio::Settings::sync();

        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .context("session bus is unavailable")?;
        bus.call_sync(
            Some("org.freedesktop.impl.portal.desktop.gnome"),
            "/org/gnome/globalshortcuts",
            "org.gnome.GlobalShortcutsRebind",
            "RebindShortcuts",
            Some(&glib::Variant::tuple_from_iter([self.app_id.to_variant(), shortcuts])),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            gio::Cancellable::NONE,
        )
        .context("GNOME did not accept the shortcuts")?;
        Ok(())
    }

    fn stored(&self) -> Vec<StoredShortcut> {
        let stored = self.application_settings().value("shortcuts");
        stored.iter().filter_map(|shortcut| StoredShortcut::from_variant(&shortcut)).collect()
    }

    fn application_settings(&self) -> gio::Settings {
        let path = format!("/org/gnome/settings-daemon/global-shortcuts/{}/", self.app_id);
        gio::Settings::with_path(Self::APPLICATION_SCHEMA, &path)
    }
}

impl GlobalShortcuts for PortalShortcuts {
    /// GNOME asks the user for new shortcuts in its own dialog.
    fn listen(&self, specs: &[ShortcutSpec], on_press: Box<dyn Fn(ShortcutPress) + Send>) -> Result<()> {
        glib::MainContext::new().block_on(Self::bind_and_listen(specs, on_press.as_ref()))?;
        Ok(())
    }

    fn accelerator(&self, spec: &ShortcutSpec) -> Option<String> {
        let shortcut = self.stored().into_iter().find(|shortcut| shortcut.id == spec.id)?;
        shortcut.accelerators.into_iter().next()
    }

    fn assign(&self, spec: &ShortcutSpec, accelerator: &str) -> Result<()> {
        self.bind(spec, vec![accelerator.to_owned()])
    }

    fn clear(&self, spec: &ShortcutSpec) -> Result<()> {
        self.bind(spec, Vec::new())
    }

    fn describe(&self, specs: &[ShortcutSpec]) -> Result<()> {
        let mut stored = self.stored();
        let mut changed = false;
        for shortcut in &mut stored {
            let Some(spec) = specs.iter().find(|spec| spec.id == shortcut.id) else {
                continue;
            };
            changed |= shortcut.description != spec.description;
            shortcut.description = spec.description.clone();
        }
        if !changed {
            return Ok(());
        }
        self.store(&stored)
    }
}
