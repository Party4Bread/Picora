#[tokio::test]
#[ignore = "Requires PICORA_GIPHY_API_KEY and live network access"]
async fn giphy_key_can_search_and_decode_a_returned_sticker() -> anyhow::Result<()> {
    let key = std::env::var("PICORA_GIPHY_API_KEY")?;
    let client = picora::providers::giphy::Giphy::new()?;
    let page = client
        .search(&key, picora::providers::giphy::Kind::Stickers, "cat", 0)
        .await?;
    let item = page
        .data
        .first()
        .ok_or_else(|| anyhow::anyhow!("GIPHY returned no stickers for the smoke-test query"))?;
    let bytes = client.image(item.image("fixed_width_still")?).await?;
    let preview = picora::media::preview(&bytes)?;
    assert!(!preview.frames.is_empty());
    Ok(())
}
