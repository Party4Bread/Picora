use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;
use url::Url;

pub const BUILD_VERSION: &str = match option_env!("PICORA_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Dev,
}

fn dev_run(version: &Version) -> Option<u64> {
    let parts: Vec<_> = version.pre.as_str().split('.').collect();
    if parts.len() != 3 || parts[0] != "dev" || !version.build.is_empty() {
        return None;
    }
    let hash = parts[2].strip_prefix('g')?;
    if hash.len() != 12 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    parts[1].parse().ok()
}

/// # Errors
/// Rejects invalid versions in release metadata or the running build.
pub fn update_available(current: &str, release: &Release, channel: Channel) -> Result<bool> {
    if release.draft {
        return Ok(false);
    }
    let candidate = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .context("Invalid release tag")?,
    )?;
    let current = Version::parse(current)?;
    match channel {
        Channel::Stable => {
            if release.prerelease || !candidate.pre.is_empty() || !candidate.build.is_empty() {
                return Ok(false);
            }
            Ok(dev_run(&current).is_some() || candidate > current)
        }
        Channel::Dev => {
            let Some(run) = dev_run(&candidate).filter(|_| release.prerelease) else {
                return Ok(false);
            };
            Ok(dev_run(&current).is_none_or(|installed| run > installed))
        }
    }
}

pub const REPOSITORY: &str = "https://github.com/Party4Bread/Picora";
pub const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_METADATA: u64 = 8 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    #[serde(rename = "windows-x86_64")]
    WindowsX86_64,
    #[serde(rename = "linux-x86_64")]
    LinuxX86_64,
    MacosAarch64,
}
impl Target {
    #[must_use]
    pub fn current() -> Option<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Some(Self::WindowsX86_64),
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("macos", "aarch64") => Some(Self::MacosAarch64),
            _ => None,
        }
    }
    #[must_use]
    pub fn asset_name(self, version: &str) -> String {
        let suffix = match self {
            Self::WindowsX86_64 => "windows-x86_64.zip",
            Self::LinuxX86_64 => "linux-x86_64.tar.gz",
            Self::MacosAarch64 => "macos-aarch64.zip",
        };
        format!("picora-{version}-{suffix}")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub targets: Vec<ReleaseAsset>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub target: Target,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default)]
    pub body: Option<String>,
    pub assets: Vec<GithubAsset>,
}
#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
    pub asset: ReleaseAsset,
    pub url: String,
}

/// # Errors
/// Rejects invalid semantic versions, tags without a `v` prefix, and non-stable release tags.
pub fn newer(current: &str, tag: &str) -> Result<bool> {
    let v = Version::parse(
        tag.strip_prefix('v')
            .context("Release tag must start with v")?,
    )?;
    ensure!(
        v.pre.is_empty() && v.build.is_empty(),
        "Only stable releases are supported"
    );
    Ok(v > Version::parse(current)?)
}
/// # Errors
/// Rejects URLs outside this project’s HTTPS GitHub release downloads, including credentials, query strings, and fragments.
pub fn asset_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url
                .path()
                .starts_with("/Party4Bread/Picora/releases/download/")
            && url.query().is_none()
            && url.fragment().is_none(),
        "Invalid release asset URL"
    );
    Ok(url)
}
fn release_asset<'a>(release: &'a Release, name: &str) -> Result<&'a GithubAsset> {
    let mut matches = release.assets.iter().filter(|a| a.name == name);
    let asset = matches.next().context("Release asset is missing")?;
    ensure!(matches.next().is_none(), "Duplicate release asset");
    let url = asset_url(&asset.browser_download_url)?;
    ensure!(
        url.path()
            == format!(
                "/Party4Bread/Picora/releases/download/{}/{name}",
                release.tag_name
            ),
        "Asset belongs to a different release"
    );
    Ok(asset)
}
/// # Errors
/// Rejects inconsistent release metadata, duplicate targets, invalid sizes or digests, and missing or mismatched assets.
pub fn select_update(
    current: &str,
    target: Target,
    release: &Release,
    manifest: Manifest,
    channel: Channel,
) -> Result<Option<AvailableUpdate>> {
    if !update_available(current, release, channel)? {
        return Ok(None);
    }
    ensure!(
        manifest.schema == 1 && release.tag_name == format!("v{}", manifest.version),
        "Release manifest version mismatch"
    );
    let mut seen = HashSet::new();
    for a in &manifest.targets {
        ensure!(
            seen.insert(a.name.clone())
                && manifest
                    .targets
                    .iter()
                    .filter(|b| b.target == a.target)
                    .count()
                    == 1,
            "Duplicate manifest target"
        );
        ensure!(
            a.name == a.target.asset_name(&manifest.version),
            "Unexpected archive name"
        );
        ensure!(
            a.size > 0
                && a.size <= MAX_ARCHIVE
                && a.sha256.len() == 64
                && hex::decode(&a.sha256).is_ok(),
            "Invalid archive size or digest"
        );
    }
    let asset = manifest
        .targets
        .into_iter()
        .find(|a| a.target == target)
        .context("This release has no build for your platform")?;
    let github = release_asset(release, &asset.name)?;
    ensure!(github.size == asset.size, "Release asset size mismatch");
    Ok(Some(AvailableUpdate {
        version: manifest.version,
        notes: release.body.clone().unwrap_or_default(),
        url: github.browser_download_url.clone(),
        asset,
    }))
}
#[derive(Clone)]
pub struct ReleaseClient {
    client: reqwest::Client,
}
impl ReleaseClient {
    /// # Errors
    /// Returns an error if the release HTTP client cannot initialize.
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Picora/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_mins(5))
            .redirect(reqwest::redirect::Policy::custom(|a| {
                let u = a.url();
                if a.previous().len() < 5
                    && u.scheme() == "https"
                    && u.username().is_empty()
                    && u.password().is_none()
                    && matches!(
                        u.host_str(),
                        Some(
                            "github.com"
                                | "release-assets.githubusercontent.com"
                                | "objects.githubusercontent.com"
                        )
                    )
                {
                    a.follow()
                } else {
                    a.stop()
                }
            }))
            .build()?;
        Ok(Self { client })
    }
    async fn metadata<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?;
        ensure!(
            response.status().is_success(),
            "Unexpected release metadata status: {}",
            response.status()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                (bytes.len() + chunk.len()) as u64 <= MAX_METADATA,
                "Release metadata too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    /// # Errors
    /// Returns an error if release metadata cannot be fetched or validated.
    pub async fn check(
        &self,
        current: &str,
        target: Target,
        channel: Channel,
    ) -> Result<Option<AvailableUpdate>> {
        let release = match channel {
            Channel::Stable => {
                match self
                    .metadata::<Release>(
                        "https://api.github.com/repos/Party4Bread/Picora/releases/latest",
                    )
                    .await
                {
                    Ok(release) => Some(release),
                    Err(error)
                        if error
                            .downcast_ref::<reqwest::Error>()
                            .and_then(reqwest::Error::status)
                            == Some(reqwest::StatusCode::NOT_FOUND) =>
                    {
                        None
                    }
                    Err(error) => {
                        return Err(error.context(
                            "Cannot check GitHub Releases; retry later if offline or rate limited",
                        ));
                    }
                }
            }
            Channel::Dev => {
                let releases: Vec<Release> = self
                    .metadata(
                        "https://api.github.com/repos/Party4Bread/Picora/releases?per_page=100",
                    )
                    .await
                    .context(
                        "Cannot check development builds; retry later if offline or rate limited",
                    )?;
                releases
                    .into_iter()
                    .filter(|release| !release.draft && release.prerelease)
                    .filter_map(|release| {
                        let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
                        Some((dev_run(&version)?, release))
                    })
                    .max_by_key(|(run, _)| *run)
                    .map(|(_, release)| release)
            }
        };
        let Some(release) = release else {
            return Ok(None);
        };
        if !update_available(current, &release, channel)? {
            return Ok(None);
        }
        let a = release_asset(&release, "update-manifest.json")?;
        let manifest = self.metadata(&a.browser_download_url).await?;
        select_update(current, target, &release, manifest, channel)
    }
    /// # Errors
    /// Rejects unsuccessful or oversized responses and size or digest mismatches; reports network and file I/O failures.
    pub async fn download(
        &self,
        update: &AvailableUpdate,
        path: &Path,
        progress: impl FnMut(u64, u64),
    ) -> Result<()> {
        asset_url(&update.url)?;
        let response = self
            .client
            .get(&update.url)
            .send()
            .await?
            .error_for_status()?;
        Self::save_response(response, update, path, progress).await
    }
    async fn save_response(
        mut response: reqwest::Response,
        update: &AvailableUpdate,
        path: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<()> {
        response.error_for_status_ref()?;
        ensure!(
            response.status().is_success(),
            "Unexpected update download status: {}",
            response.status()
        );
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await?;
        let result = async {
            let mut hash = Sha256::new();
            let mut count = 0u64;
            while let Some(chunk) = response.chunk().await? {
                count += chunk.len() as u64;
                ensure!(
                    count <= update.asset.size && count <= MAX_ARCHIVE,
                    "Download exceeds declared size"
                );
                hash.update(&chunk);
                file.write_all(&chunk).await?;
                progress(count, update.asset.size);
            }
            if count != update.asset.size
                || hex::encode(hash.finalize()) != update.asset.sha256.to_lowercase()
            {
                bail!("Update checksum or size mismatch");
            }
            file.sync_all().await?;
            Ok(())
        }
        .await;
        drop(file);
        if result.is_err() {
            let _ = tokio::fs::remove_file(path).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn server(status: u16, body: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request);
            let _ = write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(&body);
        });
        format!("http://{address}/fixture")
    }
    #[tokio::test]
    async fn metadata_errors_and_size_limits_are_reported() {
        let client = ReleaseClient::new().unwrap();
        for status in [404, 403, 429] {
            let error = client
                .metadata::<Manifest>(&server(status, b"{}".to_vec()))
                .await
                .unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<reqwest::Error>()
                    .unwrap()
                    .status()
                    .unwrap()
                    .as_u16(),
                status
            );
        }
        assert!(
            client
                .metadata::<Manifest>(&server(
                    200,
                    vec![b' '; usize::try_from(MAX_METADATA).unwrap() + 1]
                ))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn download_checks_hash_size_and_preserves_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive");
        let mut update = AvailableUpdate {
            version: "0.2.0".into(),
            notes: String::new(),
            url: String::new(),
            asset: ReleaseAsset {
                target: Target::LinuxX86_64,
                name: String::new(),
                size: 5,
                sha256: hex::encode(Sha256::digest(b"build")),
            },
        };
        let response = reqwest::get(server(200, b"build".to_vec())).await.unwrap();
        ReleaseClient::save_response(response, &update, &path, |_, _| {})
            .await
            .unwrap();
        let response = reqwest::get(server(200, b"other".to_vec())).await.unwrap();
        assert!(
            ReleaseClient::save_response(response, &update, &path, |_, _| {})
                .await
                .is_err()
        );
        assert_eq!(fs_read(&path), b"build");
        std::fs::remove_file(&path).unwrap();
        for size in [4, 5, 6] {
            update.asset.size = size;
            let response = reqwest::get(server(200, b"wrong".to_vec())).await.unwrap();
            assert!(
                ReleaseClient::save_response(response, &update, &path, |_, _| {})
                    .await
                    .is_err()
            );
            assert!(!path.exists());
        }
    }
    #[tokio::test]
    async fn stopped_redirect_is_rejected_before_creating_download() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive");
        let update = AvailableUpdate {
            version: "99.0.0".into(),
            notes: String::new(),
            url: String::new(),
            asset: ReleaseAsset {
                target: Target::LinuxX86_64,
                name: String::new(),
                size: 5,
                sha256: hex::encode(Sha256::digest(b"build")),
            },
        };
        let response = reqwest::get(server(302, b"build".to_vec())).await.unwrap();
        let error = ReleaseClient::save_response(response, &update, &path, |_, _| {
            panic!("must not write redirect body")
        })
        .await
        .unwrap_err();
        assert!(error.to_string().contains("302"));
        assert!(!path.exists());
        let client = ReleaseClient::new().unwrap();
        let error = client
            .metadata::<serde_json::Value>(&server(302, b"{}".to_vec()))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("302"));
    }
    fn fs_read(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }
}
