use std::sync::Arc;

use async_channel::Sender;
use ksni::MenuItem;
use ksni::menu::{CheckmarkItem, StandardItem};

use crate::application::desktop::LoginAutostart;
use crate::ui::action::Action;

/// Значок lexpeek в области индикаторов верхней панели.
pub struct TrayIcon {
    icon_name: String,
    autostart: Arc<dyn LoginAutostart>,
    actions: Sender<Action>,
}

impl TrayIcon {
    pub fn new(icon_name: &str, autostart: Arc<dyn LoginAutostart>, actions: Sender<Action>) -> Self {
        Self { icon_name: icon_name.to_owned(), autostart, actions }
    }

    fn send(&self, action: Action) {
        // Канал закрыт, только когда приложение уже завершается.
        let _ = self.actions.try_send(action);
    }

    fn action_item(action: Action) -> MenuItem<Self> {
        StandardItem {
            label: action.title().into(),
            activate: Box::new(move |tray: &mut Self| tray.send(action)),
            ..Default::default()
        }
        .into()
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
        self.send(Action::CaptureArea);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu: Vec<MenuItem<Self>> = Action::IN_MENU.into_iter().map(Self::action_item).collect();
        menu.push(MenuItem::Separator);
        menu.push(
            CheckmarkItem {
                label: "Запускать при входе".into(),
                checked: self.autostart.is_enabled(),
                activate: Box::new(|tray: &mut Self| tray.toggle_autostart()),
                ..Default::default()
            }
            .into(),
        );
        menu.push(MenuItem::Separator);
        menu.push(
            StandardItem {
                label: Action::Quit.title().into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Action::Quit)),
                ..Default::default()
            }
            .into(),
        );
        menu
    }
}
