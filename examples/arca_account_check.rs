//! Manual signed-in smoke check. Never prints session cookies or account details.
use picora::{
    auth,
    model::Provider,
    providers::{Repository, StickerProvider},
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cookies = auth::load()?;
    anyhow::ensure!(!cookies.is_empty(), "No saved Arca session");
    let repo = Repository::new(Provider::Arca)?;
    repo.set_cookies(&cookies);
    let catalog = repo.browse("", 1).await?;
    let found = repo.browse("고양이", 1).await?;
    anyhow::ensure!(
        catalog.iter().map(|p| &p.reference.id).collect::<Vec<_>>()
            != found.iter().map(|p| &p.reference.id).collect::<Vec<_>>(),
        "Search returned the same packs as the unfiltered catalog"
    );
    anyhow::ensure!(
        found.iter().any(|p| p.title.contains("고양이")),
        "No matching title returned"
    );
    println!(
        "Saved session verified: {} catalog packs, {} search packs, matching title found",
        catalog.len(),
        found.len()
    );
    for pack in &catalog {
        let bytes = repo.bytes(&pack.thumbnail).await?;
        picora::media::decode(&bytes)?;
    }
    println!("Decoded all {} catalog thumbnails", catalog.len());
    Ok(())
}
