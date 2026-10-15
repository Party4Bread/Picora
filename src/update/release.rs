use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;
use url::Url;

pub const REPOSITORY: &str = "https://github.com/Party4Bread/Picora";
pub const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;
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
    pub fn current() -> Option<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Some(Self::WindowsX86_64),
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("macos", "aarch64") => Some(Self::MacosAarch64),
            _ => None,
        }
    }
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
pub fn select_update(
    current: &str,
    target: Target,
    release: &Release,
    manifest: Manifest,
) -> Result<Option<AvailableUpdate>> {
    if release.draft || release.prerelease || !newer(current, &release.tag_name)? {
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
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Picora/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
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
    pub async fn check(&self, current: &str, target: Target) -> Result<Option<AvailableUpdate>> {
        let release: Release = match self
            .metadata("https://api.github.com/repos/Party4Bread/Picora/releases/latest")
            .await
        {
            Ok(r) => r,
            Err(e)
                if e.downcast_ref::<reqwest::Error>().and_then(|e| e.status())
                    == Some(reqwest::StatusCode::NOT_FOUND) =>
            {
                return Ok(None);
            }
            Err(e) => {
                return Err(e.context(
                    "Cannot check GitHub Releases; retry later if offline or rate limited",
                ));
            }
        };
        if release.draft || release.prerelease || !newer(current, &release.tag_name)? {
            return Ok(None);
        }
        let a = release_asset(&release, "update-manifest.json")?;
        let manifest = self.metadata(&a.browser_download_url).await?;
        select_update(current, target, &release, manifest)
    }
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
                .metadata::<Manifest>(&server(200, vec![b' '; MAX_METADATA as usize + 1]))
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
    fn fs_read(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }
}
