use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use crate::application::selection::{Clipboard, SelectedText};

/// Clipboard and selection through `wl-copy` and `wl-paste` from wl-clipboard.
pub struct WlClipboard;

impl Clipboard for WlClipboard {
    /// Without a press in its own window Wayland does not allow writing to the clipboard, while `wl-copy` obtains
    /// that right itself and stays in the background until the clipboard content is replaced.
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
    /// The selection lives in the primary selection; without a focused window only `wl-paste` can read it.
    fn read(&self) -> Result<Option<String>> {
        let output = Command::new("wl-paste")
            .args(["--primary", "--no-newline", "--type", "text/plain;charset=utf-8"])
            .output()
            .context("cannot run wl-paste, is wl-clipboard installed?")?;
        // wl-paste exits with an error when there is no selection.
        Ok(output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
    }
}
