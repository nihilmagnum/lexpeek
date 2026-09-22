use anyhow::{Context, Result};
use ashpd::desktop::screenshot::Screenshot;
use gtk::gio;
use gtk::prelude::FileExt;
use image::RgbaImage;

/// Снимок всего экрана через xdg-desktop-portal.
pub struct ScreenCapture;

impl ScreenCapture {
    pub async fn take() -> Result<RgbaImage> {
        let screenshot = Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await
            .context("screenshot portal is unavailable")?
            .response()
            .context("screenshot was not taken")?;
        let path = gio::File::for_uri(screenshot.uri().as_str())
            .path()
            .context("screenshot is not a local file")?;
        let image = image::open(&path).with_context(|| format!("cannot read {}", path.display()))?;
        // Портал сохраняет снимок в «Изображения», а нужен он только на время выделения.
        std::fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
        Ok(image.into_rgba8())
    }
}
