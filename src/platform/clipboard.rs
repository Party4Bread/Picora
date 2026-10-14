use crate::media::Asset;
use anyhow::{Result, anyhow};
use clipboard_rs::{Clipboard, ClipboardContext, RustImageData, common::RustImage};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct ClipboardService {
    context: Arc<Mutex<Option<ClipboardContext>>>,
}
impl ClipboardService {
    pub fn copy(&self, asset: &Asset, original: bool) -> Result<()> {
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
