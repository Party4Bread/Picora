use picora::update::release::*;
#[test]
fn versions_are_semantic_and_stable() {
    assert!(newer("0.1.9", "v0.1.10").unwrap());
    assert!(!newer("0.1.10", "v0.1.9").unwrap());
    assert!(!newer("0.1.10", "v0.1.10").unwrap());
    assert!(newer("0.1.0", "v0.2.0-beta").is_err());
}
#[test]
fn rejects_foreign_downloads_and_unsafe_names() {
    assert!(
        asset_url("https://github.com/Party4Bread/Picora/releases/download/v0.2.0/app.zip").is_ok()
    );
    for url in [
        "http://github.com/Party4Bread/Picora/releases/download/v1/x",
        "https://github.com/other/Picora/releases/download/v1/x",
        "https://evil.test/x",
    ] {
        assert!(asset_url(url).is_err());
    }
}
#[test]
fn manifest_matches_packaging_contract_and_rejects_duplicates() {
    let text = r#"{"schema":1,"version":"0.2.0","targets":[{"target":"windows-x86_64","name":"picora-0.2.0-windows-x86_64.zip","size":5,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#;
    let manifest: Manifest = serde_json::from_str(text).unwrap();
    let release = Release {
        tag_name: "v0.2.0".into(),
        draft: false,
        prerelease: false,
        body: None,
        assets: vec![GithubAsset {
            name: manifest.targets[0].name.clone(),
            size: 5,
            browser_download_url: format!(
                "{REPOSITORY}/releases/download/v0.2.0/{}",
                manifest.targets[0].name
            ),
        }],
    };
    assert!(
        select_update("0.1.0", Target::WindowsX86_64, &release, manifest.clone())
            .unwrap()
            .is_some()
    );
    assert!(select_update("0.1.0", Target::LinuxX86_64, &release, manifest.clone()).is_err());
    let mut duplicate = manifest.clone();
    duplicate.targets.push(duplicate.targets[0].clone());
    assert!(select_update("0.1.0", Target::WindowsX86_64, &release, duplicate).is_err());
    let mut wrong = manifest;
    wrong.version = "0.3.0".into();
    assert!(select_update("0.1.0", Target::WindowsX86_64, &release, wrong).is_err());
}
