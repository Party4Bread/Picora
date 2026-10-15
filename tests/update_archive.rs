use picora::update::{archive::stage_archive, release::Target};
use std::io::Write;
fn archive(entries: &[(&str, &[u8])]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    (dir, path)
}
#[test]
fn rejects_escape_and_collisions() {
    for name in [
        "../escape",
        "Picora/../escape",
        "Picora/C:/evil",
        "Picora/a:evil",
        "Picora/CON",
        "Picora/foo.",
    ] {
        let (dir, path) = archive(&[("Picora/picora.exe", b"exe"), (name, b"bad")]);
        assert!(
            stage_archive(&path, Target::WindowsX86_64, &dir.path().join("out")).is_err(),
            "{name}"
        );
        assert!(!dir.path().join("escape").exists());
    }
    let (dir, path) = archive(&[("Picora/picora.exe", b"exe"), ("Picora/PICORA.EXE", b"bad")]);
    assert!(stage_archive(&path, Target::WindowsX86_64, &dir.path().join("out")).is_err());
}
#[test]
fn valid_windows_archive_has_inventory() {
    let (dir, path) = archive(&[("Picora/picora.exe", b"exe"), ("Picora/README.md", b"read")]);
    let staged = stage_archive(&path, Target::WindowsX86_64, &dir.path().join("out")).unwrap();
    assert_eq!(staged.files.len(), 2);
    assert!(staged.root.join("picora.exe").is_file());
}
