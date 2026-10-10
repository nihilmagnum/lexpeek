use std::path::PathBuf;

use anyhow::{Context, Result};
use ashpd::desktop::screenshot::Screenshot;
use gtk::prelude::FileExt;
use gtk::{gio, glib};
use image::RgbaImage;

use crate::application::desktop::ScreenCapture;

/// Screenshot through xdg-desktop-portal.
pub struct PortalScreenCapture;

impl PortalScreenCapture {
    async fn saved_screenshot() -> Result<PathBuf> {
        let screenshot = Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await
            .context("screenshot portal is unavailable")?
            .response()
            .context("screenshot was not taken")?;
        gio::File::for_uri(screenshot.uri().as_str()).path().context("screenshot is not a local file")
    }
}

impl ScreenCapture for PortalScreenCapture {
    fn capture(&self) -> Result<RgbaImage> {
        let path = glib::MainContext::new().block_on(Self::saved_screenshot())?;
        let image = image::open(&path);
        // The portal saves the screenshot to Pictures, but it is needed only while selecting.
        std::fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
        Ok(image.with_context(|| format!("cannot read {}", path.display()))?.into_rgba8())
    }
}
