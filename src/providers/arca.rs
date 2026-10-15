//! Arca HTML adapter. Synthetic fixtures cover parsing; authenticated live verification is required.
use super::*;
use crate::model::Sticker;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub async fn browse(repo: &Repository, query: &str, page: u32) -> Result<Vec<Pack>> {
    let mut url = Url::parse("https://arca.live/e/")?;
    url.query_pairs_mut()
        .append_pair("page", &page.max(1).to_string());
    if !query.trim().is_empty() {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    parse_catalog(&repo.text(url).await?)
}
fn check_access(html: &str) -> Result<()> {
    let doc = Html::parse_document(html);
    ensure!(
        doc.select(
            &Selector::parse("form[action*='/u/login'] input[type='password'], #challenge-form")
                .unwrap()
        )
        .next()
        .is_none(),
        "Arca session expired or requires website verification. Reconnect in Accounts."
    );
    Ok(())
}
pub fn parse_catalog(html: &str) -> Result<Vec<Pack>> {
    check_access(html)?;
    let doc = Html::parse_document(html);
    let mut found = HashSet::new();
    let mut packs = Vec::new();
    for node in doc.select(&Selector::parse("a[href]").unwrap()) {
        let href = node.value().attr("href").unwrap_or_default();
        let Ok(url) = Url::parse("https://arca.live/").unwrap().join(href) else {
            continue;
        };
        let Ok(reference) = PackRef::parse(url.as_str()) else {
            continue;
        };
        if reference.provider != Provider::Arca || !found.insert(reference.key()) {
            continue;
        }
        let image = node.select(&Selector::parse("img, video").unwrap()).next();
        let title = node
            .select(&Selector::parse(".title, .name, .emoticon-title").unwrap())
            .next()
            .map(|n| n.text().collect::<String>())
            .unwrap_or_else(|| node.text().collect::<String>());
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
            || (doc
                .select(&Selector::parse(".emoticon-list, .emoticon-store").unwrap())
                .next()
                .is_some()
                && (html.contains("검색 결과가 없습니다") || html.contains("검색 결과 없음"))),
        "Arca catalog could not be read. Sign in, or check whether the website format changed."
    );
    Ok(packs)
}
fn asset_url(raw: &str) -> Result<String> {
    let url = Url::parse("https://arca.live/")?.join(raw)?;
    ensure!(allowed_url(&url), "Unsupported Arca image host");
    Ok(url.into())
}
pub async fn detail(repo: &Repository, r: &PackRef) -> Result<Pack> {
    parse_detail(&r.id, &repo.text(Url::parse(&r.url())?).await?)
}
pub fn parse_detail(id: &str, html: &str) -> Result<Pack> {
    check_access(html)?;
    let doc = Html::parse_document(html);
    let reference = PackRef::parse(&format!("https://arca.live/e/{id}"))?;
    let title = doc
        .select(&Selector::parse("article .emoticon-header .emoticon-info .title").unwrap())
        .next()
        .map(|n| n.text().collect::<String>())
        .filter(|t| !t.trim().is_empty())
        .context("Arca did not return a sticker pack. Open its source page and reconnect in Accounts if needed.")?;
    // The store navigation and recommendations also contain sticker thumbnails.
    // Only the selected pack's body is a source of clipboard assets.
    let body = doc
        .select(&Selector::parse("article .article-body.emoticon-body").unwrap())
        .next()
        .context("Arca pack content missing; the website format may have changed")?;
    let mut stickers = Vec::new();
    let mut seen = HashSet::new();
    for node in body.select(&Selector::parse("img.emoticon, video.emoticon").unwrap()) {
        let source = node.select(&Selector::parse("source[src]").unwrap()).next();
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
        let id = node
            .value()
            .attr("data-id")
            .map(str::to_owned)
            .unwrap_or_else(|| hex::encode(Sha256::digest(url.as_bytes()))[..24].into());
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
        "Arca pack has no readable sticker images. Sign in or check the website format."
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
