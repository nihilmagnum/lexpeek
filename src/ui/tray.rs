use async_channel::Sender;
use ksni::MenuItem;
use ksni::menu::{CheckmarkItem, StandardItem};

use crate::infrastructure::autostart::Autostart;

/// Действие, выбранное в значке на панели.
pub enum TrayCommand {
    Capture,
    Languages,
    ChangeShortcut,
    Quit,
}

/// Значок lexpeek в области индикаторов верхней панели.
pub struct TrayIcon {
    icon_name: String,
    autostart: Autostart,
    commands: Sender<TrayCommand>,
}

impl TrayIcon {
    pub fn new(icon_name: &str, autostart: Autostart, commands: Sender<TrayCommand>) -> Self {
        Self { icon_name: icon_name.to_owned(), autostart, commands }
    }

    fn send(&self, command: TrayCommand) {
        // Канал закрыт, только когда приложение уже завершается.
        let _ = self.commands.try_send(command);
    }

    fn toggle_autostart(&mut self) {
        if let Err(error) = self.autostart.set_enabled(!self.autostart.is_enabled()) {
            eprintln!("lexpeek: {error:#}");
        }
    }
}

impl ksni::Tray for TrayIcon {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "Lexpeek".into()
    }

    fn icon_name(&self) -> String {
        self.icon_name.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::Capture);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Перевести область".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Capture)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Языки…".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Languages)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Сменить горячую клавишу…".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::ChangeShortcut)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Запускать при входе".into(),
                checked: self.autostart.is_enabled(),
                activate: Box::new(|tray: &mut Self| tray.toggle_autostart()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Выйти".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}
