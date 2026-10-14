use picora::media::{MediaCache, decode};
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
