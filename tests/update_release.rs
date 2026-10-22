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
        select_update(
            "0.1.0",
            Target::WindowsX86_64,
            &release,
            manifest.clone(),
            Channel::Stable
        )
        .unwrap()
        .is_some()
    );
    assert!(
        select_update(
            "0.1.0",
            Target::LinuxX86_64,
            &release,
            manifest.clone(),
            Channel::Stable
        )
        .is_err()
    );
    let mut duplicate = manifest.clone();
    duplicate.targets.push(duplicate.targets[0].clone());
    assert!(
        select_update(
            "0.1.0",
            Target::WindowsX86_64,
            &release,
            duplicate,
            Channel::Stable
        )
        .is_err()
    );
    let mut wrong = manifest;
    wrong.version = "0.3.0".into();
    assert!(
        select_update(
            "0.1.0",
            Target::WindowsX86_64,
            &release,
            wrong,
            Channel::Stable
        )
        .is_err()
    );
}

#[test]
fn channel_switches_preserve_stable_isolation_and_allow_return_from_dev() {
    let mut release = Release {
        tag_name: "v0.1.8-dev.200.gabcdef123456".into(),
        draft: false,
        prerelease: true,
        body: None,
        assets: vec![],
    };
    assert!(!update_available("0.1.8", &release, Channel::Stable).unwrap());
    assert!(update_available("0.1.8", &release, Channel::Dev).unwrap());
    assert!(update_available("0.1.8-dev.100.g111111111111", &release, Channel::Dev).unwrap());
    assert!(!update_available("0.1.8-dev.200.gabcdef123456", &release, Channel::Dev).unwrap());
    assert!(!update_available("0.1.8-dev.300.g222222222222", &release, Channel::Dev).unwrap());
    release.draft = true;
    assert!(!update_available("0.1.8", &release, Channel::Dev).unwrap());
    release.draft = false;
    release.tag_name = "v0.2.0-beta.1".into();
    assert!(!update_available("0.1.8", &release, Channel::Dev).unwrap());
    release.tag_name = "v0.1.8".into();
    release.prerelease = false;
    assert!(update_available("0.2.0-dev.200.gabcdef123456", &release, Channel::Stable).unwrap());
    assert!(!update_available("0.1.7", &release, Channel::Dev).unwrap());
}

#[test]
fn dev_manifest_cannot_be_offered_to_stable_or_point_at_another_build() {
    let version = "0.1.8-dev.123.gabcdef123456";
    let name = Target::WindowsX86_64.asset_name(version);
    let manifest = Manifest {
        schema: 1,
        version: version.into(),
        targets: vec![ReleaseAsset {
            target: Target::WindowsX86_64,
            name: name.clone(),
            size: 5,
            sha256: "a".repeat(64),
        }],
    };
    let mut release = Release {
        tag_name: format!("v{version}"),
        draft: false,
        prerelease: true,
        body: None,
        assets: vec![GithubAsset {
            name: name.clone(),
            size: 5,
            browser_download_url: format!("{REPOSITORY}/releases/download/v{version}/{name}"),
        }],
    };
    assert!(
        select_update(
            "0.1.8",
            Target::WindowsX86_64,
            &release,
            manifest.clone(),
            Channel::Stable
        )
        .unwrap()
        .is_none()
    );
    assert!(
        select_update(
            "0.1.8",
            Target::WindowsX86_64,
            &release,
            manifest.clone(),
            Channel::Dev
        )
        .unwrap()
        .is_some()
    );
    release.assets[0].browser_download_url =
        format!("{REPOSITORY}/releases/download/v0.1.8/{name}");
    assert!(
        select_update(
            "0.1.8",
            Target::WindowsX86_64,
            &release,
            manifest,
            Channel::Dev
        )
        .is_err()
    );
}
