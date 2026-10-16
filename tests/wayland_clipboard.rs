#![cfg(target_os = "linux")]

use picora::{media::Asset, platform::clipboard::ClipboardService};
use std::{io::Read, time::Duration};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};

fn read(mime: &str) -> Vec<u8> {
    let (mut pipe, actual) = paste::get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(mime),
    )
    .unwrap();
    assert_eq!(actual, mime);
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
#[ignore = "requires isolated Wayland compositor with data-control; overwrites clipboard"]
fn native_wayland_image_file_replacement_and_repeated_paste() {
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_some());
    assert!(
        std::env::var_os("DISPLAY").is_none(),
        "must not pass via X11 fallback"
    );
    let root = tempfile::tempdir().unwrap();
    let still = root.path().join("still.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 128]))
        .save(&still)
        .unwrap();
    let original = root.path().join("한글 sticker #1.gif");
    std::fs::write(&original, b"original fixture").unwrap();
    let asset = Asset {
        original,
        thumbnail: still.clone(),
        still,
        animated: true,
    };
    let clipboard = ClipboardService::default();
    let png = std::fs::read(&asset.still).unwrap();
    clipboard.copy(&asset, false).unwrap();
    assert_eq!(read("image/png"), png);
    // Serving must continue after copy() returns and after the first paste.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(read("image/png"), png);
    clipboard.copy(&asset, true).unwrap();
    let uri = String::from_utf8(read("text/uri-list")).unwrap();
    assert!(uri.ends_with("\r\n"));
    assert_eq!(
        url::Url::parse(uri.trim()).unwrap().to_file_path().unwrap(),
        asset.original
    );
    assert_eq!(read("text/uri-list"), uri.as_bytes());
    clipboard.copy(&asset, false).unwrap();
    assert_eq!(read("image/png"), png);
}
