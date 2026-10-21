use super::{MAX_ASSET_BYTES, MediaCache, MediaFormat, decode, limits, video};
use anyhow::{Context, Result, ensure};
use image::{
    AnimationDecoder, ImageDecoder,
    codecs::gif::{GifEncoder, Repeat},
};
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

pub(super) enum Source {
    Video,
    Apng,
}

impl MediaCache {
    pub(super) fn clipboard_gif(&self, original: &Path, source: Source) -> Result<PathBuf> {
        ensure!(
            std::fs::metadata(original)?.len() <= MAX_ASSET_BYTES as u64,
            "Image is too large"
        );
        let bytes = std::fs::read(original)?;
        let path = original.with_file_name(format!(
            "animation-{}.gif",
            hex::encode(Sha256::digest(&bytes))
        ));
        let _guard = self
            .disk
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache lock unavailable"))?;
        let valid = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_ASSET_BYTES as u64)
            && std::fs::read(&path).is_ok_and(|bytes| {
                decode(&bytes)
                    .is_ok_and(|d| d.format == MediaFormat::Image(image::ImageFormat::Gif))
            });
        if !valid {
            let encoded = match source {
                Source::Video => video_gif(&bytes)?,
                Source::Apng => apng_gif(&bytes)?,
            };
            ensure!(
                encoded.len() <= MAX_ASSET_BYTES,
                "Converted animation is too large; use Paste original file"
            );
            let mut output =
                tempfile::NamedTempFile::new_in(path.parent().context("Missing cache directory")?)?;
            std::io::Write::write_all(&mut output, &encoded)?;
            output
                .persist(&path)
                .context("Cannot save animated clipboard file")?;
        }
        Ok(path)
    }
}

fn video_gif(bytes: &[u8]) -> Result<Vec<u8>> {
    let frames = video::decode(bytes)?;
    let mut encoded = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut encoded);
        encoder.set_repeat(Repeat::Infinite)?;
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
                image::Delay::from_numer_denom_ms(u32::try_from(delay)?, 1),
            ))?;
        }
    }
    Ok(encoded)
}

fn apng_gif(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits())?;
    let decoder = decoder.apng()?;
    let repeat = match decoder.loop_count() {
        image::metadata::LoopCount::Infinite => Repeat::Infinite,
        image::metadata::LoopCount::Finite(n) => Repeat::Finite(
            n.get()
                .try_into()
                .context("Animation repeat count is too large; use Paste original file")?,
        ),
    };
    let mut encoded = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut encoded);
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
    Ok(encoded)
}
