//! Decode bounded H.264 MP4 stickers with the statically bundled `OpenH264` codec.
use super::Frame;
use anyhow::{Context, Result, ensure};
use openh264::{
    decoder::{DecodeOptions, DecodedYUV, Decoder, Flush},
    formats::YUVSource,
};
use std::io::Cursor;

const MAX_FRAMES: u32 = 2000;
const MAX_RGBA: usize = 64 * 1024 * 1024;

pub(super) fn decode(bytes: &[u8]) -> Result<Vec<Frame>> {
    ensure!(
        bytes.len() <= super::MAX_ASSET_BYTES,
        "MP4 download is too large"
    );
    validate_boxes(bytes, 0)?;
    let mut mp4 = mp4::Mp4Reader::read_header(Cursor::new(bytes), bytes.len() as u64)
        .context("Invalid MP4 sticker")?;
    let track = mp4
        .tracks()
        .values()
        .find(|t| t.track_type().ok() == Some(mp4::TrackType::Video))
        .context("MP4 has no video track")?;
    let ValidatedTrack {
        id,
        count,
        scale,
        length_size,
        header,
    } = validate_track(track, bytes.len())?;
    let mut decoder = Decoder::new()?;
    let options = DecodeOptions::default().flush_after_decode(Flush::NoFlush);
    decoder.decode_with_options(&header, options.clone())?;
    let mut frames = Vec::new();
    let mut timings = Vec::new();
    let mut allocated = 0usize;
    for index in 1..=count {
        let sample = mp4.read_sample(id, index)?.context("Missing MP4 sample")?;
        let pts = i64::try_from(sample.start_time)?
            .checked_add(i64::from(sample.rendering_offset))
            .context("Invalid MP4 timestamp")?;
        timings.push((pts, sample.duration));
        let packet = annex_b(&sample.bytes, length_size)?;
        if let Some(yuv) = decoder.decode_with_options(&packet, options.clone())? {
            frames.push(frame(&yuv, &mut allocated)?);
        }
    }
    for yuv in decoder.flush_remaining()? {
        frames.push(frame(&yuv, &mut allocated)?);
    }
    apply_timing(&mut frames, &mut timings, scale)?;
    Ok(frames)
}

struct ValidatedTrack {
    id: u32,
    count: u32,
    scale: u64,
    length_size: usize,
    header: Vec<u8>,
}

fn validate_track(track: &mp4::Mp4Track, input_len: usize) -> Result<ValidatedTrack> {
    let avc = track
        .trak
        .mdia
        .minf
        .stbl
        .stsd
        .avc1
        .as_ref()
        .context("Unsupported MP4 codec; stickers must use H.264")?;
    ensure!(
        track.trafs.is_empty(),
        "Fragmented MP4 stickers are not supported"
    );
    let count = track.sample_count();
    ensure!(
        (1..=MAX_FRAMES).contains(&count),
        "MP4 sticker exceeds 2000 frames or is empty"
    );
    ensure!(
        track.width() > 0 && track.height() > 0 && track.width() <= 4096 && track.height() <= 4096,
        "MP4 sticker dimensions exceed 4096 pixels"
    );
    let scale = u64::from(track.timescale());
    ensure!(scale > 0, "Invalid MP4 timescale");
    ensure!(
        track.trak.mdia.mdhd.duration <= 120 * scale,
        "MP4 sticker exceeds two minutes"
    );
    validate_sample_tables(track, count, scale, input_len)?;
    let id = track.track_id();
    let length_size = usize::from((avc.avcc.length_size_minus_one & 3) + 1);
    let mut header = Vec::new();
    for nal in avc
        .avcc
        .sequence_parameter_sets
        .iter()
        .chain(&avc.avcc.picture_parameter_sets)
    {
        header.extend_from_slice(&[0, 0, 0, 1]);
        header.extend_from_slice(&nal.bytes);
    }
    ensure!(!header.is_empty(), "MP4 is missing H.264 headers");
    Ok(ValidatedTrack {
        id,
        count,
        scale,
        length_size,
        header,
    })
}

fn validate_sample_tables(
    track: &mp4::Mp4Track,
    count: u32,
    scale: u64,
    input_len: usize,
) -> Result<()> {
    let table = &track.trak.mdia.minf.stbl;
    // mp4::read_sample trusts sample tables and allocates the declared sample
    // length before reading. Validate sizes and timing before entering it.
    let sizes = &table.stsz;
    ensure!(
        sizes.sample_size as usize <= input_len
            && sizes.sample_sizes.iter().all(|n| *n as usize <= input_len),
        "Invalid MP4 sample size"
    );
    let total_size = if sizes.sample_size > 0 {
        u64::from(sizes.sample_size) * u64::from(count)
    } else {
        ensure!(
            sizes.sample_sizes.len() == count as usize,
            "Invalid MP4 sample size table"
        );
        sizes.sample_sizes.iter().map(|n| u64::from(*n)).sum()
    };
    ensure!(total_size <= input_len as u64, "Invalid MP4 sample sizes");
    let timing_count: u64 = table
        .stts
        .entries
        .iter()
        .map(|e| u64::from(e.sample_count))
        .sum();
    ensure!(
        timing_count == u64::from(count)
            && table
                .stts
                .entries
                .iter()
                .all(|e| e.sample_count > 0 && e.sample_delta > 0),
        "Invalid MP4 timing table"
    );
    let duration: u64 = table
        .stts
        .entries
        .iter()
        .map(|e| u64::from(e.sample_count) * u64::from(e.sample_delta))
        .sum();
    ensure!(duration <= 120 * scale, "MP4 sticker exceeds two minutes");
    if let Some(ctts) = &table.ctts {
        ensure!(
            ctts.entries
                .iter()
                .map(|e| u64::from(e.sample_count))
                .sum::<u64>()
                == u64::from(count),
            "Invalid MP4 composition timing"
        );
    }
    validate_chunks(track, count, input_len)
}

fn validate_chunks(track: &mp4::Mp4Track, count: u32, input_len: usize) -> Result<()> {
    let table = &track.trak.mdia.minf.stbl;
    ensure!(
        table
            .stsc
            .entries
            .iter()
            .all(|e| e.samples_per_chunk > 0 && e.samples_per_chunk <= count),
        "Invalid MP4 chunk sample count"
    );
    let chunks = table
        .stco
        .as_ref()
        .map(|t| t.entries.len())
        .or_else(|| table.co64.as_ref().map(|t| t.entries.len()))
        .unwrap_or(0);
    ensure!(
        table
            .stsc
            .entries
            .iter()
            .all(|e| e.first_chunk > 0 && e.first_chunk as usize <= chunks),
        "Invalid MP4 chunk index"
    );
    if let Some(offsets) = &table.stco {
        ensure!(
            offsets
                .entries
                .iter()
                .all(|n| u64::from(*n) < input_len as u64),
            "Invalid MP4 chunk offset"
        );
    }
    if let Some(offsets) = &table.co64 {
        ensure!(
            offsets.entries.iter().all(|n| *n < input_len as u64),
            "Invalid MP4 chunk offset"
        );
    }
    Ok(())
}

fn annex_b(mut remaining: &[u8], length_size: usize) -> Result<Vec<u8>> {
    let mut packet = Vec::new();
    while !remaining.is_empty() {
        ensure!(remaining.len() >= length_size, "Truncated H.264 NAL length");
        let len = remaining[..length_size]
            .iter()
            .fold(0usize, |n, b| (n << 8) | usize::from(*b));
        remaining = &remaining[length_size..];
        ensure!(
            len > 0 && len <= remaining.len(),
            "Invalid H.264 NAL length"
        );
        packet.extend_from_slice(&[0, 0, 0, 1]);
        packet.extend_from_slice(&remaining[..len]);
        remaining = &remaining[len..];
    }
    Ok(packet)
}

fn apply_timing(frames: &mut [Frame], timings: &mut [(i64, u32)], scale: u64) -> Result<()> {
    ensure!(
        frames.len() == timings.len(),
        "MP4 could not be fully decoded"
    );
    // OpenH264 outputs display order, while MP4 samples may be in decode order (B frames).
    timings.sort_unstable_by_key(|t| t.0);
    let origin = timings[0].0;
    for (index, frame) in frames.iter_mut().enumerate() {
        let start = u64::try_from(
            timings[index]
                .0
                .checked_sub(origin)
                .context("Invalid MP4 timestamp")?,
        )?;
        let end = if let Some(next) = timings.get(index + 1) {
            u64::try_from(
                next.0
                    .checked_sub(origin)
                    .context("Invalid MP4 timestamp")?,
            )?
        } else {
            start
                .checked_add(u64::from(timings[index].1))
                .context("Invalid MP4 duration")?
        };
        let start_ms = start.checked_mul(1000).context("Invalid MP4 duration")? / scale;
        let end_ms = end.checked_mul(1000).context("Invalid MP4 duration")? / scale;
        ensure!(
            end_ms > start_ms && end_ms <= 120_000,
            "Invalid MP4 frame timing"
        );
        frame.millis = end_ms - start_ms;
    }
    Ok(())
}

fn frame(yuv: &DecodedYUV<'_>, allocated: &mut usize) -> Result<Frame> {
    let (width, height) = yuv.dimensions();
    ensure!(
        width > 0 && height > 0 && width <= 4096 && height <= 4096,
        "Decoded MP4 dimensions exceed 4096 pixels"
    );
    let size = width * height * 4;
    *allocated += size;
    ensure!(
        *allocated <= MAX_RGBA,
        "MP4 animation exceeds 64 MiB decoded memory"
    );
    let mut rgba = vec![0; size];
    yuv.write_rgba8(&mut rgba);
    Ok(Frame {
        rgba,
        width: u32::try_from(width)?,
        height: u32::try_from(height)?,
        millis: 0,
    })
}

// Validate child boundaries before the dependency parses tables. In particular,
// zero-sized child boxes otherwise make its container loops seek back forever.
fn validate_boxes(mut bytes: &[u8], depth: usize) -> Result<()> {
    ensure!(depth <= 16, "MP4 box nesting is too deep");
    while !bytes.is_empty() {
        ensure!(bytes.len() >= 8, "Truncated MP4 box");
        let small_size = u32::from_be_bytes(bytes[..4].try_into()?);
        let (size, header) = if small_size == 1 {
            ensure!(bytes.len() >= 16, "Truncated MP4 extended box");
            (
                usize::try_from(u64::from_be_bytes(bytes[8..16].try_into()?))?,
                16,
            )
        } else {
            (small_size as usize, 8)
        };
        ensure!(
            size >= header && size <= bytes.len(),
            "Invalid MP4 box size"
        );
        let kind = &bytes[4..8];
        let payload = &bytes[header..size];
        ensure!(
            kind != b"data" || payload.len() >= 8,
            "Truncated MP4 metadata value"
        );
        ensure!(kind != b"emsg", "MP4 event messages are not supported");
        let prefix = match kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" | b"udta"
            | b"ilst" | b"\xa9nam" | b"\xa9too" | b"\xa9day" | b"covr" | b"desc" => Some(0),
            b"meta" => Some(4),
            b"stsd" | b"dref" => Some(8),
            b"avc1" | b"hev1" | b"vp09" => Some(78),
            b"mp4a" => Some(28),
            b"moof" | b"mvex" => anyhow::bail!("Fragmented MP4 stickers are not supported"),
            _ => None,
        };
        if let Some(prefix) = prefix {
            ensure!(payload.len() >= prefix, "Truncated MP4 container");
            validate_boxes(&payload[prefix..], depth + 1)?;
        }
        bytes = &bytes[size..];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_metadata_boxes_are_rejected_before_dependency_parsing() {
        assert!(super::validate_boxes(b"\x00\x00\x00\x0cdata\x00\x00\x00\x00", 0).is_err());
        assert!(super::validate_boxes(b"\x00\x00\x00\x08emsg", 0).is_err());
    }
}
