pub mod arca;
pub mod dcinside;
use crate::model::{Pack, PackRef, Provider};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{
    Client, StatusCode,
    cookie::{CookieStore, Jar},
};
use std::{sync::Arc, time::Duration};
use url::Url;

pub const MAX_DOWNLOAD: usize = 25 * 1024 * 1024;
#[derive(Clone)]
pub struct Repository {
    pub(crate) client: Client,
    pub(crate) jar: Arc<Jar>,
    pub provider: Provider,
}
impl Repository {
    pub fn new(provider: Provider) -> Result<Self> {
        let jar = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent("Mozilla/5.0 Picora/0.1")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(|a| {
                if a.previous().len() >= 5 || !allowed_url(a.url()) {
                    a.stop()
                } else {
                    a.follow()
                }
            }))
            .build()?;
        Ok(Self {
            client,
            jar,
            provider,
        })
    }
    pub fn set_cookies(&self, cookies: &[String]) {
        if let Ok(url) = Url::parse(self.provider.origin()) {
            for c in cookies {
                self.jar.add_cookie_str(c, &url);
            }
        }
    }
    pub(crate) fn cookie(&self, key: &str) -> Option<String> {
        self.jar
            .cookies(&Url::parse(self.provider.origin()).ok()?)?
            .to_str()
            .ok()?
            .split(';')
            .find_map(|c| c.trim().strip_prefix(&format!("{key}=")).map(str::to_owned))
    }
    pub async fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        let parsed = Url::parse(url)?;
        ensure!(allowed_url(&parsed), "Unsupported sticker asset host");
        let response = self
            .client
            .get(parsed)
            .header("Referer", self.provider.origin())
            .send()
            .await?;
        read_response(response).await
    }
    pub(crate) async fn text(&self, url: Url) -> Result<String> {
        let bytes = self.bytes(url.as_str()).await?;
        String::from_utf8(bytes).context("Repository returned invalid text")
    }
}
pub fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.host_str().is_some_and(|h| {
            ["dcinside.com", "arca.live", "namu.la"]
                .iter()
                .any(|d| h == *d || h.ends_with(&format!(".{d}")))
        })
}
pub(crate) async fn read_response(mut response: reqwest::Response) -> Result<Vec<u8>> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
            "Access denied. Sign in or reconnect your account; the repository may also be blocking this connection."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            bail!("Repository rate limit reached. Wait before retrying.")
        }
        _ => {
            response.error_for_status_ref()?;
        }
    }
    ensure!(
        response.content_length().unwrap_or(0) <= MAX_DOWNLOAD as u64,
        "Download exceeds 25 MiB"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= MAX_DOWNLOAD,
            "Download exceeds 25 MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
impl Repository {
    pub async fn browse(&self, query: &str, page: u32) -> Result<Vec<Pack>> {
        match self.provider {
            Provider::Dcinside => dcinside::browse(self, query, page).await,
            Provider::Arca => arca::browse(self, query, page).await,
        }
    }
    pub async fn pack(&self, r: &PackRef) -> Result<Pack> {
        ensure!(r.provider == self.provider, "Wrong repository");
        match self.provider {
            Provider::Dcinside => dcinside::detail(self, r).await,
            Provider::Arca => arca::detail(self, r).await,
        }
    }
}
