#![cfg(target_os = "windows")]

#[test]
#[ignore = "writes and deletes an isolated synthetic Windows credential"]
fn windows_credential_limit_reproduces_session_failure() {
    let account = format!("picora-test-{}", std::process::id());
    let entry = keyring::Entry::new("app.picora.test", &account).unwrap();
    entry.set_password("synthetic-session").unwrap();
    assert_eq!(entry.get_password().unwrap(), "synthetic-session");
    let result = entry.set_password(&"x".repeat(2000));
    entry.delete_credential().unwrap();
    assert!(matches!(result, Err(keyring::Error::TooLong(_, 2560))));
}
