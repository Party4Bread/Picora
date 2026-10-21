use crate::media::Asset;
use anyhow::{Result, anyhow};
use clipboard_rs::{Clipboard, ClipboardContext, RustImageData, common::RustImage};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct ClipboardService {
    context: Arc<Mutex<Option<ClipboardContext>>>,
}
impl ClipboardService {
    /// # Errors
    /// Returns an error if the asset cannot be read or clipboard ownership cannot be established. On Wayland, a failed desktop fallback preserves both backend diagnostics.
    pub fn copy(&self, asset: &Asset, original: bool) -> Result<()> {
        #[cfg(target_os = "linux")]
        if super::is_wayland() {
            match copy_wayland(asset, original) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    // Some compositors do not expose data-control. Keep the existing
                    // XWayland bridge available, but retain the native error if both fail.
                    if std::env::var_os("DISPLAY").is_none() {
                        return Err(error);
                    }
                    return self.copy_desktop(asset, original).map_err(|fallback| {
                        anyhow!("Wayland clipboard failed: {error:#}; XWayland fallback failed: {fallback:#}")
                    });
                }
            }
        }
        self.copy_desktop(asset, original)
    }

    fn copy_desktop(&self, asset: &Asset, original: bool) -> Result<()> {
        let mut guard = self
            .context
            .lock()
            .map_err(|_| anyhow!("Clipboard service unavailable"))?;
        if guard.is_none() {
            *guard =
                Some(ClipboardContext::new().map_err(|e| anyhow!("Clipboard unavailable: {e}"))?);
        }
        let clipboard = guard.as_ref().unwrap();
        if original {
            clipboard
                .set_files(vec![asset.original.to_string_lossy().into_owned()])
                .map_err(|e| anyhow!("Cannot copy file: {e}"))?;
        } else {
            let image = RustImageData::from_path(&asset.still.to_string_lossy())
                .map_err(|e| anyhow!("Cannot read sticker: {e}"))?;
            clipboard
                .set_image(image)
                .map_err(|e| anyhow!("Cannot copy image: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn copy_wayland(asset: &Asset, original: bool) -> Result<()> {
    use anyhow::Context;
    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    let (mime, bytes) = if original {
        let uri = url::Url::from_file_path(&asset.original)
            .map_err(|()| anyhow!("Invalid sticker file path"))?;
        ("text/uri-list", format!("{uri}\r\n").into_bytes())
    } else {
        (
            "image/png",
            std::fs::read(&asset.still).context("Cannot read sticker PNG")?,
        )
    };
    // Default background mode waits for ownership to be prepared, then serves
    // requests on a thread until replaced. Do not block the UI or stop after one
    // request: clipboard managers and XWayland consumers may read more than once.
    Options::new()
        .copy(
            Source::Bytes(bytes.into_boxed_slice()),
            MimeType::Specific(mime.into()),
        )
        .context(
            "Native Wayland clipboard unavailable (requires ext-data-control or wlr-data-control)",
        )
}
