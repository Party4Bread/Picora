use anyhow::{Context, Result, ensure};
use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    io::{BufReader, Cursor},
    path::PathBuf,
};

#[derive(Clone)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub millis: u64,
}
pub struct Decoded {
    pub frames: Vec<Frame>,
    pub format: ImageFormat,
}
#[derive(Debug, Clone)]
pub struct Asset {
    pub original: PathBuf,
    pub still: PathBuf,
    pub thumbnail: PathBuf,
}
#[derive(Clone)]
pub struct MediaCache {
    root: PathBuf,
    disk: std::sync::Arc<std::sync::Mutex<()>>,
}
fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(4096);
    l.max_image_height = Some(4096);
    l.max_alloc = Some(64 * 1024 * 1024);
    l
}
pub fn decode(bytes: &[u8]) -> Result<Decoded> {
    ensure!(
        bytes.len() <= super::providers::MAX_DOWNLOAD,
        "Image is too large"
    );
    let format = image::guess_format(bytes).context("Not a supported image")?;
    let reader = BufReader::new(Cursor::new(bytes));
    let animation: Option<image::Frames<'_>> = match format {
        ImageFormat::Gif => {
            let mut d = image::codecs::gif::GifDecoder::new(reader)?;
            d.set_limits(limits())?;
            Some(d.into_frames())
        }
        ImageFormat::WebP => {
            let mut d = image::codecs::webp::WebPDecoder::new(reader)?;
            d.set_limits(limits())?;
            if d.has_animation() {
                Some(d.into_frames())
            } else {
                None
            }
        }
        _ => None,
    };
    let mut frames = Vec::new();
    if let Some(animation) = animation {
        let mut total = 0usize;
        for f in animation.take(120) {
            let f = f?;
            let (num, den) = f.delay().numer_denom_ms();
            let rgba = f.into_buffer();
            total += rgba.len();
            if total > 64 * 1024 * 1024 && !frames.is_empty() {
                break;
            }
            frames.push(Frame {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
                millis: (num as u64 / den.max(1) as u64).clamp(20, 10_000),
            });
        }
    } else {
        let mut r = ImageReader::with_format(Cursor::new(bytes), format);
        r.limits(limits());
        let rgba = r.decode()?.into_rgba8();
        frames.push(Frame {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
            millis: 100,
        });
    }
    ensure!(!frames.is_empty(), "Image contains no frames");
    Ok(Decoded { frames, format })
}
impl MediaCache {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            disk: Default::default(),
        })
    }
    fn dir(&self, url: &str) -> PathBuf {
        self.root.join(hex::encode(Sha256::digest(url.as_bytes())))
    }
    pub fn cached(&self, url: &str) -> Result<Option<Asset>> {
        let _guard = self
            .disk
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
        let dir = self.dir(url);
        let manifest = dir.join("complete");
        if !manifest.exists() {
            return Ok(None);
        }
        let extension = std::fs::read_to_string(manifest)?;
        if !["png", "gif", "webp", "jpg"].contains(&extension.as_str()) {
            return Ok(None);
        }
        let asset = Asset {
            original: dir.join(format!("original.{extension}")),
            still: dir.join("still.png"),
            thumbnail: dir.join("thumb.png"),
        };
        if asset.original.exists() && asset.still.exists() && asset.thumbnail.exists() {
            if decode(&std::fs::read(&asset.original)?).is_err()
                || decode(&std::fs::read(&asset.still)?).is_err()
            {
                return Ok(None);
            }
            Ok(Some(asset))
        } else {
            Ok(None)
        }
    }
    pub fn save(&self, url: &str, bytes: &[u8]) -> Result<Asset> {
        let decoded = decode(bytes)?;
        let _guard = self
            .disk
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
        let extension = match decoded.format {
            ImageFormat::Png => "png",
            ImageFormat::Gif => "gif",
            ImageFormat::WebP => "webp",
            ImageFormat::Jpeg => "jpg",
            _ => anyhow::bail!("Unsupported sticker image format"),
        };
        let dir = self.dir(url);
        std::fs::create_dir_all(&dir)?;
        let asset = Asset {
            original: dir.join(format!("original.{extension}")),
            still: dir.join("still.png"),
            thumbnail: dir.join("thumb.png"),
        };
        let first = &decoded.frames[0];
        let image = image::RgbaImage::from_raw(first.width, first.height, first.rgba.clone())
            .context("Invalid image dimensions")?;
        image.save(&asset.still)?;
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(160, 160)
            .save(&asset.thumbnail)?;
        std::fs::write(&asset.original, bytes)?;
        // Commit marker is written last. Interrupted downloads are never treated as complete.
        std::fs::write(dir.join("complete"), extension)?;
        Ok(asset)
    }
}

/// UI previews use bounded texture dimensions; original clipboard assets remain untouched.
pub fn preview(bytes: &[u8]) -> Result<Decoded> {
    let mut decoded = decode(bytes)?;
    for frame in &mut decoded.frames {
        if frame.width > 160 || frame.height > 160 {
            let image = image::RgbaImage::from_raw(
                frame.width,
                frame.height,
                std::mem::take(&mut frame.rgba),
            )
            .context("Invalid preview dimensions")?;
            let thumb = image::DynamicImage::ImageRgba8(image)
                .thumbnail(160, 160)
                .into_rgba8();
            frame.width = thumb.width();
            frame.height = thumb.height();
            frame.rgba = thumb.into_raw();
        }
    }
    Ok(decoded)
}
