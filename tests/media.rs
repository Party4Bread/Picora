use picora::media::{CopyMode, MediaCache, decode};
#[test]
fn rejects_corrupt_images_and_preserves_transparency() {
    assert!(decode(b"not an image").is_err());
    let mut bytes = std::io::Cursor::new(Vec::new());
    let rgba = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 0]));
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let decoded = decode(bytes.get_ref()).unwrap();
    assert_eq!(decoded.frames[0].rgba[3], 0);
    assert_eq!(decoded.frames[0].width, 2);
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let asset = cache.save("static", bytes.get_ref()).unwrap();
    assert!(!cache.prepare_copy(asset, CopyMode::Auto).unwrap().1);
}
#[test]
fn gif_frames_and_cache_original_extension_are_preserved() {
    let mut bytes = Vec::new();
    {
        let mut enc = image::codecs::gif::GifEncoder::new(&mut bytes);
        for c in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            enc.encode_frame(image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 2, image::Rgba(c)),
                0,
                0,
                image::Delay::from_numer_denom_ms(100, 1),
            ))
            .unwrap();
        }
    }
    assert_eq!(decode(&bytes).unwrap().frames.len(), 2);
    let temp = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(temp.path().to_owned()).unwrap();
    let asset = cache.save("https://example.invalid/a", &bytes).unwrap();
    assert_eq!(asset.original.extension().unwrap(), "gif");
    assert!(asset.still.exists());
    assert!(cache.prepare_copy(asset.clone(), CopyMode::Auto).unwrap().1);
    assert!(
        !cache
            .prepare_copy(asset.clone(), CopyMode::Still)
            .unwrap()
            .1
    );
    assert_eq!(
        cache
            .cached("https://example.invalid/a")
            .unwrap()
            .unwrap()
            .original,
        asset.original
    );
}

#[test]
fn apng_preview_and_default_paste_preserve_animation() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        encoder.set_frame_delay(1, 10).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255].repeat(4))
            .unwrap();
        writer
            .write_image_data(&[0, 255, 0, 255].repeat(4))
            .unwrap();
        writer.finish().unwrap();
    }
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.frames.len(), 2);
    assert!(decoded.animated);
    assert_eq!(decoded.frames[1].millis, 100);
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let asset = cache.save("apng", &bytes).unwrap();
    assert!(cache.cached("apng").unwrap().unwrap().animated);
    let (paste, is_file) = cache.prepare_copy(asset.clone(), CopyMode::Auto).unwrap();
    assert!(is_file);
    assert_eq!(paste.original.extension().unwrap(), "gif");
    let gif = decode(&std::fs::read(&paste.original).unwrap()).unwrap();
    assert_eq!(gif.frames.len(), 2);
    assert_eq!(gif.frames[1].millis, 100);
    assert_eq!(&gif.frames[1].rgba[..4], &[0, 255, 0, 255]);
    std::fs::write(&paste.original, b"corrupted derivative").unwrap();
    let (repaired, _) = cache.prepare_copy(asset.clone(), CopyMode::Auto).unwrap();
    assert_eq!(
        decode(&std::fs::read(repaired.original).unwrap())
            .unwrap()
            .frames
            .len(),
        2
    );
    let (original, is_file) = cache.prepare_copy(asset, CopyMode::Original).unwrap();
    assert!(is_file);
    assert_eq!(std::fs::read(original.original).unwrap(), bytes);
}
#[test]
fn incomplete_or_corrupted_cache_is_a_miss_not_a_permanent_error() {
    let temp = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(temp.path().to_owned()).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let asset = cache.save("key", bytes.get_ref()).unwrap();
    std::fs::write(
        asset.original.parent().unwrap().join("complete"),
        "../../bad",
    )
    .unwrap();
    assert!(cache.cached("key").unwrap().is_none());
    cache.save("key", bytes.get_ref()).unwrap();
    std::fs::write(&asset.still, b"corrupt").unwrap();
    assert!(cache.cached("key").unwrap().is_none());
}
#[test]
fn concurrent_import_and_preview_leave_a_readable_cached_asset() {
    let temp = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(temp.path().to_owned()).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(128, 128)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = std::sync::Arc::new(bytes.into_inner());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let cache = cache.clone();
            let bytes = bytes.clone();
            std::thread::spawn(move || cache.save("same-url", &bytes).unwrap())
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let asset = cache.cached("same-url").unwrap().unwrap();
    assert_eq!(
        decode(&std::fs::read(asset.still).unwrap()).unwrap().frames[0].width,
        128
    );
}

#[test]
fn mp4_stickers_preview_and_copy_as_animation_or_original() {
    let bytes = include_bytes!("fixtures/animation.mp4");
    let decoded = decode(bytes).unwrap();
    assert!(decoded.animated);
    assert_eq!(decoded.frames.len(), 12);
    assert_eq!(decoded.frames[0].width, 32);
    assert_eq!(decoded.frames.iter().map(|f| f.millis).sum::<u64>(), 1000);
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let asset = cache.save("video", bytes).unwrap();
    assert_eq!(asset.original.extension().unwrap(), "mp4");
    assert!(cache.cached("video").unwrap().unwrap().animated);
    let (paste, file) = cache.prepare_copy(asset.clone(), CopyMode::Auto).unwrap();
    assert!(file);
    assert_eq!(paste.original.extension().unwrap(), "gif");
    let gif = decode(&std::fs::read(&paste.original).unwrap()).unwrap();
    assert_eq!(gif.frames.len(), 12);
    assert_ne!(gif.frames[0].rgba, gif.frames[11].rgba);
    let duration = gif.frames.iter().map(|f| f.millis).sum::<u64>();
    assert!(duration.abs_diff(1000) <= 10, "GIF duration: {duration}");
    assert!(
        !cache
            .prepare_copy(asset.clone(), CopyMode::Still)
            .unwrap()
            .1
    );
    assert_eq!(
        std::fs::read(
            cache
                .prepare_copy(asset, CopyMode::Original)
                .unwrap()
                .0
                .original
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn mp4_frames_match_independent_reference_in_display_order() {
    let reference = image::load_from_memory(include_bytes!("fixtures/animation-reference.png"))
        .unwrap()
        .to_rgba8();
    let decoded = decode(include_bytes!("fixtures/animation.mp4")).unwrap();
    for (index, frame) in decoded.frames.iter().enumerate() {
        let mut error = 0u64;
        for y in 0..32 {
            for x in 0..32 {
                let expected = reference.get_pixel(index as u32 * 32 + x, y);
                for channel in 0..3 {
                    error += u64::from(
                        frame.rgba[((y * 32 + x) * 4) as usize + channel]
                            .abs_diff(expected[channel]),
                    );
                }
            }
        }
        assert!(
            error / (32 * 32 * 3) < 4,
            "frame {index}: RGB error {error}"
        );
    }
}

#[test]
fn malformed_mp4_timing_and_sample_sizes_are_errors() {
    let original = include_bytes!("fixtures/animation.mp4");
    for (kind, offset, value, expected) in [
        (b"mdhd", 16, 0u32, "timescale"),
        (b"stts", 12, 0, "timing"),
        (b"stsc", 12, 0, "chunk"),
        (b"stsz", 8, original.len() as u32 + 1, "sample size"),
    ] {
        let mut bytes = original.to_vec();
        let pos = bytes.windows(4).position(|b| b == kind).unwrap() + offset;
        bytes[pos..pos + 4].copy_from_slice(&value.to_be_bytes());
        let error = decode(&bytes).err().expect("malformed media must fail");
        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }
    assert!(decode(&original[..original.len() / 2]).is_err());
}

#[test]
fn mp4_zero_sized_child_is_rejected_without_hanging() {
    let bytes = b"\x00\x00\x00\x10ftypisom\x00\x00\x00\x00\x00\x00\x00\x10moov\x00\x00\x00\x00free";
    assert!(decode(bytes).is_err());
}

#[test]
fn cache_persists_integrity_metadata_and_migrates_legacy_entries() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let bytes = include_bytes!("fixtures/animation.mp4");
    let asset = cache.save("metadata", bytes).unwrap();
    let metadata = asset.original.with_file_name("metadata.json");
    assert!(
        metadata.exists(),
        "warm cache hits must not decode animation again"
    );
    std::fs::remove_file(&metadata).unwrap();
    let legacy = cache.cached("metadata").unwrap().unwrap();
    assert!(legacy.animated);
    assert!(metadata.exists(), "legacy entries should migrate once");
    let mut corrupted = std::fs::read(&asset.original).unwrap();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 1;
    std::fs::write(&asset.original, corrupted).unwrap();
    assert!(
        cache.cached("metadata").unwrap().is_none(),
        "same-length corruption must be detected"
    );
}

#[test]
fn still_preview_does_not_read_or_decode_the_original_animation() {
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let asset = cache
        .save("still-preview", include_bytes!("fixtures/animation.mp4"))
        .unwrap();
    std::fs::remove_file(&asset.original).unwrap();
    let decoded = picora::media::preview_asset(&asset, false).unwrap();
    assert_eq!(decoded.frames.len(), 1);
    assert!(decoded.frames[0].width <= 160);
    assert!(picora::media::preview_asset(&asset, true).is_err());
}
