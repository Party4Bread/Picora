#![cfg(target_os = "macos")]

use clipboard_rs::{Clipboard, ClipboardContext, common::RustImage};
use picora::{
    media::{CopyMode, MediaCache, decode},
    platform::clipboard::ClipboardService,
};

#[test]
#[ignore = "changes the native macOS clipboard; requires a desktop session"]
fn macos_video_clipboard_preserves_animation_original_and_still_image() {
    let temp = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(temp.path().to_owned()).unwrap();
    let bytes = include_bytes!("fixtures/animation.mp4");
    let asset = cache.save("fixture", bytes).unwrap();
    let writer = ClipboardService::default();
    let reader = ClipboardContext::new().unwrap();
    // Finish with an image, so the clipboard does not reference a deleted temp file.
    for mode in [CopyMode::Auto, CopyMode::Original, CopyMode::Still] {
        let (copy, file) = cache.prepare_copy(asset.clone(), mode).unwrap();
        writer.copy(&copy, file).unwrap();
        if file {
            assert_eq!(
                reader.get_files().unwrap(),
                vec![copy.original.to_string_lossy().into_owned()]
            );
            let copied = std::fs::read(&copy.original).unwrap();
            assert_eq!(decode(&copied).unwrap().frames.len(), 12);
            match mode {
                CopyMode::Auto => assert!(copied.starts_with(b"GIF")),
                CopyMode::Original => assert_eq!(copied, bytes),
                CopyMode::Still => unreachable!(),
            }
        } else {
            assert_eq!(reader.get_image().unwrap().get_size(), (32, 32));
        }
    }
}
