use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ashpd::desktop::PersistMode;
use ashpd::desktop::remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions};
use ashpd::enumflags2::BitFlags;
use gtk::glib;

use crate::application::selection::PasteKeystroke;

/// Нажатия клавиш в активном окне через портал удалённого управления GNOME.
pub struct RemoteKeyboard {
    /// Токен, по которому GNOME восстанавливает разрешение без повторного вопроса.
    restore_token: PathBuf,
}

impl RemoteKeyboard {
    /// Коды evdev, а не символы: так вставка не зависит от раскладки.
    const LEFT_CTRL: i32 = 29;
    const V: i32 = 47;

    pub fn new(restore_token: PathBuf) -> Self {
        Self { restore_token }
    }

    async fn press_paste(&self) -> Result<()> {
        let portal = RemoteDesktop::new().await.context("remote desktop portal is unavailable")?;
        let session = portal.create_session(Default::default()).await?;
        let token = fs::read_to_string(&self.restore_token).ok();
        let devices = SelectDevicesOptions::default()
            .set_devices(BitFlags::from(DeviceType::Keyboard))
            .set_persist_mode(PersistMode::ExplicitlyRevoked)
            .set_restore_token(token.as_deref());
        portal.select_devices(&session, devices).await?.response()?;
        let started = portal.start(&session, None, Default::default()).await?;
        let started = started.response().context("remote control was not allowed")?;
        if let Some(token) = started.restore_token() {
            fs::write(&self.restore_token, token)
                .with_context(|| format!("cannot write {}", self.restore_token.display()))?;
        }

        let keystrokes = [
            (Self::LEFT_CTRL, KeyState::Pressed),
            (Self::V, KeyState::Pressed),
            (Self::V, KeyState::Released),
            (Self::LEFT_CTRL, KeyState::Released),
        ];
        for (key, state) in keystrokes {
            portal.notify_keyboard_keycode(&session, key, state, Default::default()).await?;
        }
        session.close().await?;
        Ok(())
    }
}

impl PasteKeystroke for RemoteKeyboard {
    fn paste(&self) -> Result<()> {
        glib::MainContext::new().block_on(self.press_paste())
    }
}
