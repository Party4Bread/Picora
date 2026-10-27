use super::selector;
use super::{Context, Pack, PackRef, Provider, Repository, Result, Url, allowed_url, ensure};
use crate::model::Sticker;
use rust_i18n::t;
use scraper::Html;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// # Errors
/// Returns an error on network failure, a sign-in challenge, or an unrecognized catalog response.
pub async fn browse(repo: &Repository, query: &str, page: u32) -> Result<Vec<Pack>> {
    parse_catalog(&repo.text(catalog_url(query, page)?).await?)
}
fn catalog_url(query: &str, page: u32) -> Result<Url> {
    let mut url = Url::parse("https://arca.live/e/")?;
    url.query_pairs_mut()
        .append_pair("p", &page.max(1).to_string());
    if !query.trim().is_empty() {
        url.query_pairs_mut()
            .append_pair("target", "title")
            .append_pair("keyword", query.trim());
    }
    Ok(url)
}
fn check_access(html: &str) -> Result<()> {
    let doc = Html::parse_document(html);
    ensure!(
        doc.select(&selector(
            "form[action*='/u/login'] input[type='password'], #challenge-form"
        )?)
        .next()
        .is_none(),
        "{}",
        t!("arca.live session expired or requires website verification. Reconnect in Accounts.")
    );
    Ok(())
}
/// # Errors
/// Rejects sign-in challenges and pages without a recognizable catalog or explicit empty-result state.
pub fn parse_catalog(html: &str) -> Result<Vec<Pack>> {
    check_access(html)?;
    let doc = Html::parse_document(html);
    let base = Url::parse("https://arca.live/")?;
    let direct_media = selector(":scope > img, :scope > video")?;
    let mut found = HashSet::new();
    let mut packs = Vec::new();
    for node in doc.select(&selector(".emoticon-list a[href]")?) {
        // Arca repeats a popular-pack ranking above every query and page.
        // Exclude it before deduplication: a ranked pack can also be a real match.
        if node
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .any(|parent| parent.value().classes().any(|class| class == "rank-list"))
        {
            continue;
        }
        let href = node.value().attr("href").unwrap_or_default();
        let Ok(url) = base.join(href) else {
            continue;
        };
        let Ok(reference) = PackRef::parse(url.as_str()) else {
            continue;
        };
        if reference.provider != Provider::Arca || !found.insert(reference.key()) {
            continue;
        }
        // Rating badges are nested before the sticker image in live cards.
        // Select the card's direct media, not the first descendant image.
        let image = node
            .select(&selector(".emoticon > img, .emoticon > video")?)
            .next()
            .or_else(|| node.select(&direct_media).next());
        let title = node
            .select(&selector(".title, .name, .emoticon-title")?)
            .next()
            .map_or_else(
                || node.text().collect::<String>(),
                |n| n.text().collect::<String>(),
            );
        let title = if title.trim().is_empty() {
            image
                .and_then(|i| i.value().attr("alt"))
                .unwrap_or("Untitled pack")
                .into()
        } else {
            title.trim().into()
        };
        let thumbnail = image
            .and_then(|n| n.value().attr("data-src").or(n.value().attr("src")))
            .and_then(|s| asset_url(s).ok())
            .unwrap_or_default();
        packs.push(Pack {
            url: reference.url(),
            reference,
            title,
            author: String::new(),
            thumbnail,
            stickers: Vec::new(),
        });
    }
    ensure!(
        !packs.is_empty()
            || doc
                .select(&selector(".emoticon-list > .row:not(.rank-list)")?)
                .next()
                .is_some()
            || (doc
                .select(&selector(".emoticon-list, .emoticon-store")?)
                .next()
                .is_some()
                && (html.contains("검색 결과가 없습니다") || html.contains("검색 결과 없음"))),
        "{}",
        t!(
            "arca.live catalog could not be read. Sign in, or check whether the website format changed."
        )
    );
    Ok(packs)
}
fn asset_url(raw: &str) -> Result<String> {
    let url = Url::parse("https://arca.live/")?.join(raw)?;
    ensure!(allowed_url(&url), "Unsupported arca.live image host");
    Ok(url.into())
}
/// # Errors
/// Returns an error if the pack cannot be fetched or its page cannot be parsed.
pub async fn detail(repo: &Repository, r: &PackRef) -> Result<Pack> {
    parse_detail(&r.id, &repo.text(Url::parse(&r.url())?).await?)
}
/// # Errors
/// Rejects invalid pack identifiers, access challenges, missing pack content, unsupported asset hosts, and packs with no readable stickers.
pub fn parse_detail(id: &str, html: &str) -> Result<Pack> {
    check_access(html)?;
    let doc = Html::parse_document(html);
    let reference = PackRef::parse(&format!("https://arca.live/e/{id}"))?;
    let title = doc
        .select(&selector("article .emoticon-header .emoticon-info .title")?)
        .next()
        .map(|n| n.text().collect::<String>())
        .filter(|t| !t.trim().is_empty())
        .with_context(|| t!("arca.live did not return a sticker pack. Open its source page and reconnect in Accounts if needed.").into_owned())?;
    // The store navigation and recommendations also contain sticker thumbnails.
    // Only the selected pack's body is a source of clipboard assets.
    let body = doc
        .select(&selector("article .article-body.emoticon-body")?)
        .next()
        .with_context(|| {
            t!("arca.live pack content missing; the website format may have changed").into_owned()
        })?;
    let mut stickers = Vec::new();
    let mut seen = HashSet::new();
    for node in body.select(&selector("img.emoticon, video.emoticon")?) {
        let source = node.select(&selector("source[src]")?).next();
        let Some(raw) = node
            .value()
            .attr("data-src")
            .or(node.value().attr("src"))
            .or_else(|| source.and_then(|n| n.value().attr("src")))
        else {
            continue;
        };
        let url = asset_url(raw)?;
        if !seen.insert(url.clone()) {
            continue;
        }
        let id = node.value().attr("data-id").map_or_else(
            || hex::encode(Sha256::digest(url.as_bytes()))[..24].into(),
            str::to_owned,
        );
        stickers.push(Sticker {
            id,
            label: node
                .value()
                .attr("alt")
                .filter(|s| !s.is_empty())
                .unwrap_or("Sticker")
                .into(),
            url,
        });
    }
    ensure!(
        !stickers.is_empty(),
        "{}",
        t!("arca.live pack has no readable sticker images. Sign in or check the website format.")
    );
    Ok(Pack {
        url: reference.url(),
        reference,
        title: title.trim().into(),
        author: String::new(),
        thumbnail: stickers[0].url.clone(),
        stickers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_uses_arca_form_fields_and_preserves_korean_text() {
        let url = catalog_url("  고양이 & 강아지+ #1  ", 2).unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(
            pairs.get("keyword").map(std::convert::AsRef::as_ref),
            Some("고양이 & 강아지+ #1")
        );
        assert_eq!(
            pairs.get("target").map(std::convert::AsRef::as_ref),
            Some("title")
        );
        assert_eq!(pairs.get("p").map(std::convert::AsRef::as_ref), Some("2"));
        assert_eq!(pairs.len(), 3);
        assert!(url.fragment().is_none());
    }
}
