use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use crate::application::selection::{Clipboard, SelectedText};

/// Буфер обмена и выделение через `wl-copy` и `wl-paste` из wl-clipboard.
pub struct WlClipboard;

impl Clipboard for WlClipboard {
    /// Без нажатия в собственном окне Wayland не даёт записать в буфер обмена, а `wl-copy` получает
    /// это право сам и остаётся в фоне, пока содержимое буфера не заменят.
    fn put(&self, text: &str) -> Result<()> {
        let mut copy = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .context("cannot run wl-copy, is wl-clipboard installed?")?;
        copy.stdin.take().context("wl-copy has no stdin")?.write_all(text.as_bytes())?;
        let status = copy.wait()?;
        ensure!(status.success(), "wl-copy failed with {status}");
        Ok(())
    }
}

impl SelectedText for WlClipboard {
    /// Выделение хранится в primary selection; без окна в фокусе его читает только `wl-paste`.
    fn read(&self) -> Result<Option<String>> {
        let output = Command::new("wl-paste")
            .args(["--primary", "--no-newline", "--type", "text/plain;charset=utf-8"])
            .output()
            .context("cannot run wl-paste, is wl-clipboard installed?")?;
        // wl-paste завершается с ошибкой, когда выделения нет.
        Ok(output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
    }
}
