use picora::{
    media::decode,
    model::{PackRef, Provider},
    providers::{Repository, StickerProvider},
};
#[tokio::test]
#[ignore = "contacts DCInside; run explicitly"]
async fn live_dcinside_search_pack_and_image() {
    let repo = Repository::new(Provider::Dcinside).unwrap();
    let packs = repo.browse("고양이", 1).await.unwrap();
    assert!(!packs.is_empty());
    let reference = PackRef::parse("https://dccon.dcinside.com/#41982").unwrap();
    let pack = repo.pack(&reference).await.unwrap();
    assert_eq!(pack.reference, reference);
    let bytes = repo.bytes(&pack.stickers[0].url).await.unwrap();
    assert!(!decode(&bytes).unwrap().frames.is_empty());
}

/// Session input is a private JSON array of Set-Cookie strings; never print it.
#[tokio::test]
#[ignore = "requires PICORA_ARCA_COOKIES_FILE and authenticated Arca access"]
async fn live_arca_video_pack_preview_cache_and_clipboard_assets() {
    use picora::media::{CopyMode, MediaCache};
    let file = std::env::var("PICORA_ARCA_COOKIES_FILE").expect("private session file required");
    let cookies: Vec<String> = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let repo = Repository::new(Provider::Arca).unwrap();
    repo.set_cookies(&cookies);
    let reference = PackRef::parse("https://arca.live/e/54950").unwrap();
    let pack = repo.pack(&reference).await.unwrap();
    assert_eq!(pack.title, "실비아 정실력측정");
    assert_eq!(pack.stickers.len(), 22);
    let temp = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(temp.path().to_owned()).unwrap();
    for sticker in pack.stickers {
        let bytes = repo.bytes(&sticker.url).await.unwrap();
        let asset = cache.save(&sticker.url, &bytes).unwrap();
        assert!(asset.animated);
        let frames = decode(&bytes).unwrap().frames;
        assert!(frames.len() > 1);
        assert!(cache.cached(&sticker.url).unwrap().unwrap().animated);
        let (copy, file) = cache.prepare_copy(asset.clone(), CopyMode::Auto).unwrap();
        assert!(file);
        let gif = decode(&std::fs::read(copy.original).unwrap()).unwrap();
        assert_eq!(gif.frames.len(), frames.len());
        assert!(gif.frames.len() > 1);
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
        println!(
            "Verified sticker {}: {} animated frames",
            sticker.id,
            frames.len()
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "run under xvfb-run with xclip installed; changes that display's clipboard"]
fn mp4_native_x11_clipboard_smoke() {
    use picora::{
        media::{CopyMode, MediaCache},
        platform::clipboard::ClipboardService,
    };
    let dir = tempfile::tempdir().unwrap();
    let cache = MediaCache::new(dir.path().to_owned()).unwrap();
    let asset = cache
        .save("fixture", include_bytes!("fixtures/animation.mp4"))
        .unwrap();
    let clipboard = ClipboardService::default();
    for mode in [CopyMode::Auto, CopyMode::Original, CopyMode::Still] {
        let (copy, file) = cache.prepare_copy(asset.clone(), mode).unwrap();
        clipboard.copy(&copy, file).unwrap();
        let mime = if file { "text/uri-list" } else { "image/png" };
        let result = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-o", "-t", mime])
            .output()
            .unwrap();
        assert!(result.status.success());
        if file {
            let text = String::from_utf8(result.stdout).unwrap();
            let path = url::Url::parse(text.trim())
                .unwrap()
                .to_file_path()
                .unwrap();
            assert_eq!(path, copy.original);
            assert_eq!(
                decode(&std::fs::read(path).unwrap()).unwrap().frames.len(),
                12
            );
        } else {
            assert_eq!(decode(&result.stdout).unwrap().frames[0].width, 32);
        }
    }
}
