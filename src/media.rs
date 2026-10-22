mod video;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaFormat {
    Image(ImageFormat),
    Mp4,
}
pub struct Decoded {
    pub frames: Vec<Frame>,
    pub format: MediaFormat,
    pub animated: bool,
}
#[derive(Debug, Clone)]
pub struct Asset {
    pub original: PathBuf,
    pub still: PathBuf,
    pub thumbnail: PathBuf,
    pub animated: bool,
}
#[derive(Clone, Copy)]
pub enum CopyMode {
    Auto,
    Still,
    Original,
}
#[derive(Clone)]
pub struct MediaCache {
    root: PathBuf,
    disk: std::sync::Arc<std::sync::Mutex<()>>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheMetadata {
    extension: String,
    animated: bool,
    hashes: [String; 3],
}
fn asset_hashes(asset: &Asset) -> Result<[String; 3]> {
    let hash = |path: &PathBuf| -> Result<String> {
        ensure!(
            std::fs::metadata(path)?.len() <= super::providers::MAX_DOWNLOAD as u64,
            "Cached image is too large"
        );
        Ok(hex::encode(Sha256::digest(std::fs::read(path)?)))
    };
    Ok([
        hash(&asset.original)?,
        hash(&asset.still)?,
        hash(&asset.thumbnail)?,
    ])
}
fn save_metadata(asset: &Asset, extension: &str, hashes: [String; 3]) -> Result<()> {
    let metadata = CacheMetadata {
        extension: extension.into(),
        animated: asset.animated,
        hashes,
    };
    let mut file = tempfile::NamedTempFile::new_in(
        asset.original.parent().context("Missing cache directory")?,
    )?;
    std::io::Write::write_all(&mut file, &serde_json::to_vec(&metadata)?)?;
    file.persist(asset.original.with_file_name("metadata.json"))?;
    Ok(())
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
    if bytes.get(4..8) == Some(b"ftyp") {
        let mut frames = video::decode(bytes)?;
        frames.truncate(120);
        return Ok(Decoded {
            frames,
            format: MediaFormat::Mp4,
            animated: true,
        });
    }
    let format = image::guess_format(bytes).context("Not a supported image")?;
    let reader = BufReader::new(Cursor::new(bytes));
    let animation: Option<image::Frames<'_>> = match format {
        ImageFormat::Png => {
            let mut d = image::codecs::png::PngDecoder::new(reader)?;
            d.set_limits(limits())?;
            if d.is_apng()? {
                Some(d.apng()?.into_frames())
            } else {
                None
            }
        }
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
    // Preview limits may keep only one frame of a large animation. Preserve
    // source animation metadata independently of that bounded preview.
    let animated = animation.is_some();
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
    Ok(Decoded {
        frames,
        format: MediaFormat::Image(format),
        animated,
    })
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
        if !["png", "gif", "webp", "jpg", "mp4"].contains(&extension.as_str()) {
            return Ok(None);
        }
        let mut asset = Asset {
            original: dir.join(format!("original.{extension}")),
            still: dir.join("still.png"),
            thumbnail: dir.join("thumb.png"),
            animated: false,
        };
        let Ok(hashes) = asset_hashes(&asset) else {
            return Ok(None);
        };
        if let Ok(bytes) = std::fs::read(dir.join("metadata.json"))
            && let Ok(metadata) = serde_json::from_slice::<CacheMetadata>(&bytes)
        {
            if metadata.extension != extension || metadata.hashes != hashes {
                return Ok(None);
            }
            asset.animated = metadata.animated;
            return Ok(Some(asset));
        }
        // Old entries have no metadata. Validate once, then migrate without
        // changing the original bytes or the legacy completion marker.
        let Ok(decoded) = decode(&std::fs::read(&asset.original)?) else {
            return Ok(None);
        };
        if decode(&std::fs::read(&asset.still)?).is_err()
            || decode(&std::fs::read(&asset.thumbnail)?).is_err()
        {
            return Ok(None);
        }
        asset.animated = decoded.animated;
        save_metadata(&asset, &extension, hashes)?;
        Ok(Some(asset))
    }

    pub fn save(&self, url: &str, bytes: &[u8]) -> Result<Asset> {
        let decoded = decode(bytes)?;
        let _guard = self
            .disk
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
        let extension = match decoded.format {
            MediaFormat::Image(ImageFormat::Png) => "png",
            MediaFormat::Image(ImageFormat::Gif) => "gif",
            MediaFormat::Image(ImageFormat::WebP) => "webp",
            MediaFormat::Image(ImageFormat::Jpeg) => "jpg",
            MediaFormat::Mp4 => "mp4",
            _ => anyhow::bail!("Unsupported sticker image format"),
        };
        let dir = self.dir(url);
        std::fs::create_dir_all(&dir)?;
        // Invalidate the old commit before replacing any files.
        match std::fs::remove_file(dir.join("complete")) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let asset = Asset {
            original: dir.join(format!("original.{extension}")),
            still: dir.join("still.png"),
            thumbnail: dir.join("thumb.png"),
            animated: decoded.animated,
        };
        let first = &decoded.frames[0];
        let image = image::RgbaImage::from_raw(first.width, first.height, first.rgba.clone())
            .context("Invalid image dimensions")?;
        image.save(&asset.still)?;
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(160, 160)
            .save(&asset.thumbnail)?;
        std::fs::write(&asset.original, bytes)?;
        save_metadata(&asset, extension, asset_hashes(&asset)?)?;
        // Commit marker is written last. Interrupted downloads are never treated as complete.
        std::fs::write(dir.join("complete"), extension)?;
        Ok(asset)
    }

    pub fn prepare_copy(&self, mut asset: Asset, mode: CopyMode) -> Result<(Asset, bool)> {
        let file = match mode {
            CopyMode::Still => false,
            CopyMode::Original => true,
            CopyMode::Auto => {
                asset.animated || asset.original.extension().is_some_and(|e| e == "gif")
            }
        };
        if matches!(mode, CopyMode::Auto) && asset.original.extension().is_some_and(|e| e == "mp4")
        {
            let bytes = std::fs::read(&asset.original)?;
            let path = asset.original.with_file_name(format!(
                "animation-{}.gif",
                hex::encode(Sha256::digest(&bytes))
            ));
            let _guard = self
                .disk
                .lock()
                .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
            let valid = std::fs::read(&path).is_ok_and(|b| {
                decode(&b).is_ok_and(|d| d.format == MediaFormat::Image(ImageFormat::Gif))
            });
            if !valid {
                let frames = video::decode(&bytes)?;
                let mut encoded = Vec::new();
                {
                    let mut encoder = image::codecs::gif::GifEncoder::new(&mut encoded);
                    encoder.set_repeat(image::codecs::gif::Repeat::Infinite)?;
                    let mut elapsed = 0u64;
                    let mut written_ticks = 0u64;
                    for frame in frames {
                        elapsed += frame.millis;
                        let ticks = (elapsed + 5) / 10;
                        let delay = ticks.saturating_sub(written_ticks).max(1) * 10;
                        written_ticks += delay / 10;
                        encoder.encode_frame(image::Frame::from_parts(
                            image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
                                .context("Invalid video frame dimensions")?,
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(delay as u32, 1),
                        ))?;
                    }
                }
                ensure!(
                    encoded.len() <= super::providers::MAX_DOWNLOAD,
                    "Converted animation is too large"
                );
                let mut output = tempfile::NamedTempFile::new_in(
                    path.parent().context("Missing cache directory")?,
                )?;
                std::io::Write::write_all(&mut output, &encoded)?;
                output
                    .persist(&path)
                    .context("Cannot save animated clipboard file")?;
            }
            asset.original = path;
        }
        if matches!(mode, CopyMode::Auto)
            && asset.animated
            && asset.original.extension().is_some_and(|e| e == "png")
        {
            let bytes = std::fs::read(&asset.original)?;
            let path = asset.original.with_file_name(format!(
                "animation-{}.gif",
                hex::encode(Sha256::digest(&bytes))
            ));
            let _guard = self
                .disk
                .lock()
                .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
            let valid = std::fs::metadata(&path)
                .is_ok_and(|m| m.len() <= super::providers::MAX_DOWNLOAD as u64)
                && std::fs::read(&path).is_ok_and(|bytes| {
                    decode(&bytes).is_ok_and(|d| d.format == MediaFormat::Image(ImageFormat::Gif))
                });
            if !valid {
                let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
                decoder.set_limits(limits())?;
                let decoder = decoder.apng()?;
                let repeat = match decoder.loop_count() {
                    image::metadata::LoopCount::Infinite => image::codecs::gif::Repeat::Infinite,
                    image::metadata::LoopCount::Finite(n) => {
                        image::codecs::gif::Repeat::Finite(n.get().try_into().context(
                            "Animation repeat count is too large; use Paste original file",
                        )?)
                    }
                };
                let mut encoded = Vec::new();
                {
                    let mut encoder = image::codecs::gif::GifEncoder::new(&mut encoded);
                    encoder.set_repeat(repeat)?;
                    let mut total = 0usize;
                    for (index, frame) in decoder.into_frames().enumerate() {
                        let frame = frame?;
                        total += frame.buffer().len();
                        ensure!(
                            index < 2000 && total <= 64 * 1024 * 1024,
                            "Animation is too large to convert; use Paste original file"
                        );
                        encoder.encode_frame(frame)?;
                    }
                }
                ensure!(
                    encoded.len() <= super::providers::MAX_DOWNLOAD,
                    "Converted animation is too large; use Paste original file"
                );
                let mut file = tempfile::NamedTempFile::new_in(
                    path.parent().context("Missing cache directory")?,
                )?;
                std::io::Write::write_all(&mut file, &encoded)?;
                file.persist(&path)
                    .context("Cannot save animated clipboard file")?;
            }
            asset.original = path;
        }
        Ok((asset, file))
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

/// Idle thumbnails never decode the original animation. Hover previews do.
pub fn preview_asset(asset: &Asset, animate: bool) -> Result<Decoded> {
    preview(&std::fs::read(if animate {
        &asset.original
    } else {
        &asset.thumbnail
    })?)
}
