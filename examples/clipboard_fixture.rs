//! Native clipboard fixture for browser paste smoke tests. Changes the clipboard.
use anyhow::{Result, bail};
use picora::{
    media::{CopyMode, MediaCache},
    platform::clipboard::ClipboardService,
};
use std::io::Write;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    if command.as_deref() == Some("library") {
        let root = std::path::PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing test data directory"))?,
        );
        std::fs::create_dir_all(&root)?;
        let cache = MediaCache::new(root.join("media"))?;
        let url = "https://fixture.picora.invalid/animation.mp4";
        cache.save(url, include_bytes!("../tests/fixtures/animation.mp4"))?;
        let store = picora::store::Store::open(&root.join("library.sqlite3"))?;
        let reference = picora::model::PackRef {
            provider: picora::model::Provider::Arca,
            id: "0".into(),
        };
        store.save_favorite_pack(&picora::model::Pack {
            url: reference.url(),
            reference,
            title: "Browser paste fixture".into(),
            author: "Picora test".into(),
            thumbnail: url.into(),
            stickers: vec![picora::model::Sticker {
                id: "1".into(),
                label: "Animation fixture".into(),
                url: url.into(),
            }],
        })?;
        store.save_settings(&picora::model::Settings {
            check_updates_automatically: false,
            shortcut: "Ctrl+Alt+Space".into(),
            ..Default::default()
        })?;
        println!("{}", root.display());
        return Ok(());
    }
    let mode = match command.as_deref() {
        Some("auto") => CopyMode::Auto,
        Some("original") => CopyMode::Original,
        Some("still") => CopyMode::Still,
        _ => bail!("Usage: clipboard_fixture auto|original|still CACHE_DIRECTORY"),
    };
    let root = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing cache directory"))?;
    let cache = MediaCache::new(root.into())?;
    let asset = cache.save(
        "browser-fixture",
        include_bytes!("../tests/fixtures/animation.mp4"),
    )?;
    let (copy, file) = cache.prepare_copy(asset, mode)?;
    let writer = ClipboardService::default();
    writer.copy(&copy, file)?;
    println!(
        "{}",
        if file {
            copy.original.display()
        } else {
            copy.still.display()
        }
    );
    std::io::stdout().flush()?;
    if args.next().as_deref() == Some("--hold") {
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    Ok(())
}
