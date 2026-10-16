//! Repeatable local media-path benchmark; no network or user library access.
use picora::media::{MediaCache, preview};
use std::time::Instant;
fn main() -> anyhow::Result<()> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for i in 0..120 {
            encoder.encode_frame(image::Frame::from_parts(
                image::RgbaImage::from_pixel(160, 160, image::Rgba([i, 100, 200, 255])),
                0,
                0,
                image::Delay::from_numer_denom_ms(100, 1),
            ))?;
        }
    }
    let dir = tempfile::tempdir()?;
    let cache = MediaCache::new(dir.path().to_owned())?;
    let asset = cache.save("profile", &bytes)?;
    let start = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(cache.cached("profile")?);
    }
    println!("20 warm cache lookups: {:?}", start.elapsed());
    let start = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(preview(&std::fs::read(&asset.original)?)?);
    }
    println!("20 full animation previews: {:?}", start.elapsed());
    let start = Instant::now();
    for _ in 0..20 {
        std::hint::black_box(preview(&std::fs::read(&asset.thumbnail)?)?);
    }
    println!("20 still thumbnails: {:?}", start.elapsed());
    Ok(())
}
