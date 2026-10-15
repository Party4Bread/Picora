use picora::update::{archive::StagedUpdate, release::Target, transaction::*};
use std::{fs, path::PathBuf};
#[test]
fn replace_rollback_and_commit_preserve_unrelated_files() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("install");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("picora"), b"old").unwrap();
    fs::write(root.join("unins000.exe"), b"keep").unwrap();
    let stage = d.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("picora"), b"new").unwrap();
    let staged = StagedUpdate {
        root: stage,
        files: vec![PathBuf::from("picora")],
        target: Target::LinuxX86_64,
    };
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    let mut tx = Transaction::prepare(&layout, &staged).unwrap();
    tx.apply().unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"new");
    tx.rollback().unwrap();
    tx.rollback().unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("unins000.exe")).unwrap(), b"keep");
    drop(tx);
    let mut tx = Transaction::prepare(&layout, &staged).unwrap();
    tx.apply().unwrap();
    tx.commit().unwrap();
    drop(tx);
    recover(&layout).unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"new");
}
#[test]
fn interrupted_replacement_is_recovered() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("install");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("picora"), b"old").unwrap();
    let stage = d.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("picora"), b"new").unwrap();
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    let staged = StagedUpdate {
        root: stage,
        files: vec!["picora".into()],
        target: Target::LinuxX86_64,
    };
    {
        let mut tx = Transaction::prepare(&layout, &staged).unwrap();
        tx.apply().unwrap();
    }
    recover(&layout).unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"old");
}
#[test]
fn existing_files_remain_present_when_preparing_backup() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("install");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("picora"), b"old").unwrap();
    let stage = d.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("picora"), b"new").unwrap();
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    let staged = StagedUpdate {
        root: stage,
        files: vec!["picora".into()],
        target: Target::LinuxX86_64,
    };
    let tx = Transaction::prepare(&layout, &staged).unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"old");
    assert_eq!(
        fs::read(layout.state_dir().unwrap().join("backup/picora")).unwrap(),
        b"old"
    );
    drop(tx);
    recover(&layout).unwrap();
}
#[test]
fn startup_gate_covers_preparation_before_a_journal_exists() {
    let d = tempfile::tempdir().unwrap();
    let layout = InstallLayout::for_root(d.path().to_owned(), Target::LinuxX86_64).unwrap();
    let gate = layout.startup_gate().unwrap();
    assert!(!layout.state_dir().unwrap().join("journal.json").exists());
    assert!(layout.startup_gate().is_err());
    drop(gate);
    assert!(layout.startup_gate().is_ok());
}
#[test]
fn partial_apply_failure_restores_earlier_files() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("app");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("picora"), b"old").unwrap();
    fs::write(root.join("blocked"), b"user file").unwrap();
    let stage = d.path().join("staged");
    fs::create_dir_all(stage.join("blocked")).unwrap();
    fs::write(stage.join("picora"), b"new").unwrap();
    fs::write(stage.join("blocked/file"), b"new").unwrap();
    let staged = StagedUpdate {
        root: stage,
        target: Target::LinuxX86_64,
        files: vec!["picora".into(), "blocked/file".into()],
    };
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    let mut tx = Transaction::prepare(&layout, &staged).unwrap();
    assert!(tx.apply().is_err());
    tx.rollback().unwrap();
    assert_eq!(fs::read(root.join("picora")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("blocked")).unwrap(), b"user file");
}
#[test]
fn old_inventory_removes_only_obsolete_owned_files() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("app");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("picora"), b"old").unwrap();
    fs::write(root.join("obsolete.txt"), b"old").unwrap();
    fs::write(root.join("unrelated.txt"), b"keep").unwrap();
    fs::write(
        root.join(".picora-files.json"),
        br#"["picora","obsolete.txt"]"#,
    )
    .unwrap();
    let stage = d.path().join("staged");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("picora"), b"new").unwrap();
    let staged = StagedUpdate {
        root: stage,
        target: Target::LinuxX86_64,
        files: vec!["picora".into()],
    };
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    let mut tx = Transaction::prepare(&layout, &staged).unwrap();
    tx.apply().unwrap();
    tx.commit().unwrap();
    assert!(!root.join("obsolete.txt").exists());
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"keep");
}
#[cfg(unix)]
#[test]
fn read_only_installation_is_rejected_before_handoff() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("app");
    fs::create_dir(&root).unwrap();
    let layout = InstallLayout::for_root(root.clone(), Target::LinuxX86_64).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();
    let result = layout.check_writable();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(!layout.state_dir().unwrap().join("journal.json").exists());
}
