use super::selector;
use super::{Context, Pack, PackRef, Repository, Result, Url, ensure, read_response};
use crate::model::Sticker;
use scraper::Html;
use serde_json::Value;

/// # Errors
/// Returns an error if the catalog request fails or its response cannot be parsed.
pub async fn browse(repo: &Repository, query: &str, page: u32) -> Result<Vec<Pack>> {
    let mut url = Url::parse("https://dccon.dcinside.com/")?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL"))?;
        path.clear()
            .push(if query.trim().is_empty() {
                "new"
            } else {
                "hot"
            })
            .push(&page.max(1).to_string());
        if !query.trim().is_empty() {
            path.push("title").push(query.trim());
        }
    }
    parse_catalog(&repo.text(url).await?)
}
/// # Errors
/// Rejects catalogs with missing titles or without recognizable pack-list markup.
pub fn parse_catalog(html: &str) -> Result<Vec<Pack>> {
    let doc = Html::parse_document(html);
    let item = selector(".div_package[package_idx]")?;
    let title = selector(".dcon_name")?;
    let author = selector(".dcon_seller")?;
    let img = selector("img")?;
    let mut result = Vec::new();
    for node in doc.select(&item) {
        let Some(id) = node.value().attr("package_idx") else {
            continue;
        };
        let reference = PackRef::parse(&format!("https://dccon.dcinside.com/#{id}"))?;
        let name = node
            .select(&title)
            .next()
            .map(|n| n.text().collect::<String>())
            .unwrap_or_default();
        ensure!(
            !name.trim().is_empty(),
            "DCInside catalog format changed: missing title"
        );
        result.push(Pack {
            url: reference.url(),
            reference,
            title: name.trim().into(),
            author: node
                .select(&author)
                .next()
                .map(|n| n.text().collect::<String>())
                .unwrap_or_default(),
            thumbnail: node
                .select(&img)
                .next()
                .and_then(|n| n.value().attr("src"))
                .unwrap_or_default()
                .into(),
            stickers: Vec::new(),
        });
    }
    ensure!(
        !result.is_empty() || doc.select(&selector(".dccon_shop_list")?).next().is_some(),
        "DCInside catalog format changed or access was blocked"
    );
    Ok(result)
}
/// # Errors
/// Returns an error if session initialization, the pack request, or detail parsing fails.
pub async fn detail(repo: &Repository, r: &PackRef) -> Result<Pack> {
    repo.text(Url::parse(repo.provider.origin())?).await?;
    let token = repo
        .cookie("ci_c")
        .context("DCInside did not provide its request token")?;
    let response = repo
        .client
        .post("https://dccon.dcinside.com/index/package_detail")
        .header("Referer", r.url())
        .header("X-Requested-With", "XMLHttpRequest")
        .form(&[
            ("ci_t", token.as_str()),
            ("package_idx", r.id.as_str()),
            ("code", ""),
            ("inspection_state", ""),
        ])
        .send()
        .await?;
    let pack = parse_detail(&String::from_utf8(read_response(response).await?)?)?;
    ensure!(pack.reference == *r, "DCInside returned a different pack");
    Ok(pack)
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("DCInside response missing {key}"))
}
/// # Errors
/// Rejects invalid JSON, missing pack metadata, or a pack with no readable stickers.
pub fn parse_detail(body: &str) -> Result<Pack> {
    let v: Value = serde_json::from_str(body)
        .context("DCInside pack response changed or the pack is unavailable")?;
    let info = &v["info"];
    let reference = PackRef::parse(&format!(
        "https://dccon.dcinside.com/#{}",
        required(info, "package_idx")?
    ))?;
    let mut stickers = Vec::new();
    for s in v["detail"]
        .as_array()
        .context("DCInside response missing stickers")?
    {
        let mut url = Url::parse("https://dcimg5.dcinside.com/dccon.php")?;
        url.query_pairs_mut()
            .append_pair("no", required(s, "path")?);
        stickers.push(Sticker {
            id: required(s, "idx")?.into(),
            label: s["title"].as_str().unwrap_or("Sticker").into(),
            url: url.into(),
        });
    }
    ensure!(!stickers.is_empty(), "This pack has no available stickers");
    Ok(Pack {
        url: reference.url(),
        reference,
        title: required(info, "title")?.into(),
        author: info["seller_name"].as_str().unwrap_or_default().into(),
        thumbnail: stickers[0].url.clone(),
        stickers,
    })
}
