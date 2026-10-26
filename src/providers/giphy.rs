//! GIPHY search returns individual items with a user-supplied key. Items the
//! user keeps (favorites or group members) are stored in one collection pack.
use crate::model::{Pack, PackRef, Provider, Sticker};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};
use url::Url;

pub const PAGE_SIZE: u32 = 24;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stickers,
    Gifs,
}
impl Kind {
    fn path(self) -> &'static str {
        match self {
            Self::Stickers => "stickers",
            Self::Gifs => "gifs",
        }
    }
}
#[derive(Clone, Deserialize)]
pub struct Rendition {
    #[serde(default)]
    pub url: String,
}
#[derive(Clone, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub username: String,
    pub images: HashMap<String, Rendition>,
}
impl Item {
    /// Uses GIPHY's size-capped rendition so search grids and clipboard copies stay light.
    /// # Errors
    /// Rejects malformed identifiers and items without a usable GIF rendition.
    pub fn sticker(&self) -> Result<Sticker> {
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 64
                && self.id.bytes().all(|b| b.is_ascii_alphanumeric()),
            "GIPHY returned an invalid item"
        );
        let label = [self.title.trim(), self.username.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("GIPHY");
        Ok(Sticker {
            id: self.id.clone(),
            label: label.into(),
            url: self
                .image("downsized")
                .or_else(|_| self.image("original"))?
                .into(),
        })
    }
    pub fn image(&self, rendition: &str) -> Result<&str> {
        let url = &self
            .images
            .get(rendition)
            .context("GIPHY did not provide this image format")?
            .url;
        validate_url(url)?;
        Ok(url)
    }
}
#[derive(Deserialize)]
pub struct Page {
    pub data: Vec<Item>,
    pub pagination: Pagination,
}
impl Page {
    #[must_use]
    pub fn more(&self) -> bool {
        self.pagination.offset.saturating_add(self.pagination.count) < self.pagination.total_count
    }
    /// Items without a usable rendition are skipped rather than failing the page.
    #[must_use]
    pub fn stickers(&self) -> Vec<Sticker> {
        self.data.iter().filter_map(|i| i.sticker().ok()).collect()
    }
}
/// The library pack holding every GIPHY item the user kept.
#[must_use]
pub fn collection() -> Pack {
    let reference = PackRef {
        provider: Provider::Giphy,
        id: "saved".into(),
    };
    Pack {
        url: reference.url(),
        reference,
        title: "Saved from GIPHY".into(),
        author: "GIPHY".into(),
        thumbnail: String::new(),
        stickers: Vec::new(),
    }
}
#[derive(Deserialize)]
pub struct Pagination {
    pub total_count: u32,
    pub count: u32,
    pub offset: u32,
}

#[derive(Clone)]
pub struct Giphy {
    client: Client,
}
impl Giphy {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }
    pub async fn search(&self, key: &str, kind: Kind, query: &str, offset: u32) -> Result<Page> {
        let url = search_url(key, kind, query, offset)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(reqwest::Error::without_url)?;
        let bytes = response_bytes(response, 2 * 1024 * 1024).await?;
        let page: Page =
            serde_json::from_slice(&bytes).context("GIPHY returned an unreadable response")?;
        ensure!(
            page.data.len() <= PAGE_SIZE as usize,
            "GIPHY returned too many results"
        );
        Ok(page)
    }
    pub async fn image(&self, url: &str) -> Result<Vec<u8>> {
        let url = validate_url(url)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(reqwest::Error::without_url)?;
        response_bytes(response, crate::media::MAX_ASSET_BYTES).await
    }
}
fn search_url(key: &str, kind: Kind, query: &str, offset: u32) -> Result<Url> {
    ensure!(
        !key.trim().is_empty(),
        "Add your GIPHY API key in Accounts first"
    );
    let endpoint = if query.trim().is_empty() {
        "trending"
    } else {
        "search"
    };
    let mut url = Url::parse(&format!(
        "https://api.giphy.com/v1/{}/{endpoint}",
        kind.path()
    ))?;
    url.query_pairs_mut()
        .append_pair("api_key", key.trim())
        .append_pair("limit", &PAGE_SIZE.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair("rating", "g");
    if endpoint == "search" {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    Ok(url)
}
pub fn validate_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).context("Invalid GIPHY media address")?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url
                .host_str()
                .is_some_and(|h| h == "giphy.com" || h.ends_with(".giphy.com")),
        "GIPHY returned an unsupported media address"
    );
    Ok(url)
}
async fn response_bytes(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
            "GIPHY rejected the API key. Check your key and its access in the GIPHY developer dashboard."
        ),
        StatusCode::TOO_MANY_REQUESTS => bail!(
            "GIPHY's request limit was reached. Wait before trying again or check your API plan."
        ),
        status => ensure!(status.is_success(), "GIPHY request failed ({status})"),
    }
    ensure!(
        response.content_length().is_none_or(|n| n <= limit as u64),
        "GIPHY response is too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(reqwest::Error::without_url)?
    {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= limit,
            "GIPHY response is too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::tests::response;

    #[test]
    fn search_accepts_video_only_renditions_alongside_images() {
        let page: Page = serde_json::from_value(serde_json::json!({
            "data": [{
                "id": "cat", "url": "https://giphy.com/gifs/cat",
                "images": {
                    "original": {"url": "https://media.giphy.com/cat.gif?cid=example"},
                    "looping": {"mp4": "https://media.giphy.com/cat.mp4"},
                    "preview": {"mp4": "https://media.giphy.com/preview.mp4"},
                    "downsized_small": {"mp4": "https://media.giphy.com/small.mp4"}
                }
            }],
            "pagination": {"total_count": 1, "count": 1, "offset": 0}
        }))
        .unwrap();
        assert_eq!(
            page.data[0].image("original").unwrap(),
            "https://media.giphy.com/cat.gif?cid=example"
        );
        assert!(page.data[0].image("looping").is_err());
        let sticker = page.data[0].sticker().unwrap();
        assert_eq!(sticker.url, "https://media.giphy.com/cat.gif?cid=example");
        assert_eq!(sticker.label, "GIPHY");
    }

    #[test]
    fn items_become_size_capped_stickers_and_skip_unusable_entries() {
        let page: Page = serde_json::from_value(serde_json::json!({
            "data": [
                {
                    "id": "abc123", "title": "  ", "username": "catmaker",
                    "images": {
                        "original": {"url": "https://media.giphy.com/full.gif"},
                        "downsized": {"url": "https://media.giphy.com/small.gif"}
                    }
                },
                {"id": "../x", "images": {"original": {"url": "https://media.giphy.com/x.gif"}}},
                {"id": "evil", "images": {"original": {"url": "https://evil.example/x.gif"}}}
            ],
            "pagination": {"total_count": 30, "count": 3, "offset": 24}
        }))
        .unwrap();
        let stickers = page.stickers();
        assert_eq!(stickers.len(), 1);
        assert_eq!(stickers[0].id, "abc123");
        assert_eq!(stickers[0].label, "catmaker");
        assert_eq!(stickers[0].url, "https://media.giphy.com/small.gif");
        assert!(page.more());
        let collection = collection();
        assert_eq!(collection.reference.key(), "giphy:saved");
        assert!(collection.stickers.is_empty());
    }

    #[tokio::test]
    async fn api_boundaries_reject_auth_errors_redirects_and_oversized_bodies() {
        for (status, expected) in [(401, "API key"), (429, "request limit"), (302, "failed")] {
            let reply = response(status, "", b"sensitive upstream details".to_vec()).await;
            let error = response_bytes(reply, 32).await.unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("sensitive upstream details"));
        }
        let reply = response(200, "", vec![b'x'; 33]).await;
        assert!(
            response_bytes(reply, 32)
                .await
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        for raw in [
            "https://giphy.com.evil.example/x",
            "https://giphy.com@evil.example/x",
            "http://media.giphy.com/x",
            "file:///tmp/x",
        ] {
            assert!(validate_url(raw).is_err());
        }
        let url = search_url("private-key", Kind::Stickers, "고양이 & #1", 24).unwrap();
        let params: HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params.get("q").map(|v| v.as_ref()), Some("고양이 & #1"));
        assert_eq!(params.get("offset").map(|v| v.as_ref()), Some("24"));
        assert_eq!(url.path(), "/v1/stickers/search");
        assert!(url.fragment().is_none());
    }
}
