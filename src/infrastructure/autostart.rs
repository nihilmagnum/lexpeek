use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::desktop::LoginAutostart;

/// Автозапуск через ярлык в ~/.config/autostart.
pub struct DesktopEntryAutostart {
    app_id: String,
    entry: PathBuf,
    binary: PathBuf,
}

impl DesktopEntryAutostart {
    pub fn new(config_dir: &Path, app_id: &str, binary: PathBuf) -> Self {
        Self {
            app_id: app_id.to_owned(),
            entry: config_dir.join("autostart").join(format!("{app_id}.desktop")),
            binary,
        }
    }

    fn desktop_entry(&self) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Lexpeek\n\
             Exec=\"{}\" --background\n\
             Icon={}\n\
             Terminal=false\n\
             NoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\n",
            self.binary.display(),
            self.app_id
        )
    }
}

impl LoginAutostart for DesktopEntryAutostart {
    fn is_enabled(&self) -> bool {
        self.entry.exists()
    }

    fn set_enabled(&self, enabled: bool) -> Result<()> {
        if !enabled {
            return fs::remove_file(&self.entry).with_context(|| format!("cannot remove {}", self.entry.display()));
        }
        if let Some(directory) = self.entry.parent() {
            fs::create_dir_all(directory).with_context(|| format!("cannot create {}", directory.display()))?;
        }
        fs::write(&self.entry, self.desktop_entry()).with_context(|| format!("cannot write {}", self.entry.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_entry_that_starts_in_background() {
        let config = tempfile::tempdir().unwrap();
        let autostart = DesktopEntryAutostart::new(config.path(), "test.App", PathBuf::from("/opt/lexpeek"));

        autostart.set_enabled(true).unwrap();
        let entry = fs::read_to_string(config.path().join("autostart/test.App.desktop")).unwrap();
        assert!(entry.contains("Exec=\"/opt/lexpeek\" --background"));

        autostart.set_enabled(false).unwrap();
        assert!(!autostart.is_enabled());
    }
}
