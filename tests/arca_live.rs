use picora::{
    auth,
    model::Provider,
    providers::{Repository, StickerProvider},
};
#[tokio::test]
#[ignore = "requires a saved Arca session and live repository access"]
async fn saved_session_search_and_thumbnails() -> anyhow::Result<()> {
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
        !found.is_empty() && found.iter().all(|p| p.title.contains("고양이")),
        "Search included unrelated titles or returned no matches"
    );
    println!(
        "Saved session verified: {} catalog packs, {} search packs, matching title found",
        catalog.len(),
        found.len()
    );
    let dogs = repo.browse("강아지", 1).await?;
    anyhow::ensure!(
        !dogs.is_empty() && dogs.iter().all(|p| p.title.contains("강아지")),
        "Unrelated dog search results"
    );
    let empty = repo.browse("picora_no_match_726364", 1).await?;
    anyhow::ensure!(empty.is_empty(), "No-match search returned packs");
    let next = repo.browse("고양이", 2).await?;
    anyhow::ensure!(
        !next.is_empty() && next.iter().all(|p| p.title.contains("고양이")),
        "Unrelated second-page results"
    );
    anyhow::ensure!(
        next.iter()
            .all(|p| !found.iter().any(|first| first.reference == p.reference)),
        "Pagination repeated packs"
    );
    println!("Verified two queries, an empty search, and page two");
    for pack in &catalog {
        let bytes = repo.bytes(&pack.thumbnail).await?;
        picora::media::decode(&bytes)?;
    }
    println!("Decoded all {} catalog thumbnails", catalog.len());
    Ok(())
}
