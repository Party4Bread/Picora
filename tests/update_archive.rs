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
#[test]
fn rejects_zip_links_and_missing_executable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("link.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.add_symlink(
        "Picora/picora.exe",
        "../../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    assert!(stage_archive(&path, Target::WindowsX86_64, &dir.path().join("out")).is_err());
    let (dir, path) = archive(&[("Picora/README.md", b"read")]);
    assert!(stage_archive(&path, Target::WindowsX86_64, &dir.path().join("out")).is_err());
}
#[test]
fn rejects_tar_links_and_declared_expansion_bombs() {
    for link in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.tar.gz");
        let mut gzip = flate2::write::GzEncoder::new(
            std::fs::File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut header = tar::Header::new_gnu();
        header.set_path("Picora/picora").unwrap();
        header.set_mode(0o755);
        if link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("../../escape").unwrap();
            header.set_size(0);
        } else {
            header.set_size(1024 * 1024 * 1024 + 1);
        }
        header.set_cksum();
        gzip.write_all(header.as_bytes()).unwrap();
        gzip.write_all(&[0; 1024]).unwrap();
        gzip.finish().unwrap();
        assert!(stage_archive(&path, Target::LinuxX86_64, &dir.path().join("out")).is_err());
    }
}
