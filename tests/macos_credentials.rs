#![cfg(target_os = "macos")]

#[test]
#[ignore = "writes and deletes an isolated synthetic macOS Keychain credential"]
fn macos_keychain_roundtrips_large_sessions_and_deletes_them() {
    let account = format!("picora-test-{}", std::process::id());
    let entry = keyring::Entry::new("app.picora.test", &account).unwrap();
    struct Cleanup(keyring::Entry);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.delete_credential();
        }
    }
    let cleanup = Cleanup(entry);
    let cookies = vec![format!(
        "synthetic-session={}; Path=/; Secure",
        "x".repeat(16_384)
    )];
    let session = serde_json::to_string(&cookies).unwrap();
    cleanup.0.set_password(&session).unwrap();
    let reopened = keyring::Entry::new("app.picora.test", &account).unwrap();
    assert_eq!(reopened.get_password().unwrap(), session);
    reopened.set_password("replacement-session").unwrap();
    assert_eq!(cleanup.0.get_password().unwrap(), "replacement-session");
    cleanup.0.delete_credential().unwrap();
    assert!(matches!(
        reopened.get_password(),
        Err(keyring::Error::NoEntry)
    ));
}
