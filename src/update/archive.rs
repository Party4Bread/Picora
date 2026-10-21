use super::release::Target;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpdate {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub target: Target,
}

/// # Errors
/// Rejects absolute paths, traversal, invalid components, and Windows drive or stream syntax.
pub fn safe_relative(name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && !name.contains(['\\', ':', '\0']) && !name.starts_with('/'),
        "Unsafe archive path"
    );
    for part in name.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != ".." && !part.ends_with(['.', ' ']),
            "Unsafe path component"
        );
        let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(base.len() == 4
                    && (base.starts_with("COM") || base.starts_with("LPT"))
                    && base.as_bytes()[3].is_ascii_digit()),
            "Reserved filename"
        );
    }
    Ok(PathBuf::from(name))
}
/// # Errors
/// Rejects unsafe or oversized archive entries, duplicate paths, links, missing executables, and extraction failures.
pub fn stage_archive(path: &Path, target: Target, destination: &Path) -> Result<StagedUpdate> {
    ensure!(!destination.exists(), "Staging directory already exists");
    fs::create_dir(destination)?;
    let result = extract(path, target, destination);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}
fn extract(path: &Path, target: Target, destination: &Path) -> Result<StagedUpdate> {
    let mut output = Extraction {
        destination,
        seen: HashSet::new(),
        files: Vec::new(),
        total: 0,
        entries: 0,
    };
    if target == Target::LinuxX86_64 {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(path)?));
        for entry in archive.entries()? {
            let mut entry = entry?;
            let kind = entry.header().entry_type();
            ensure!(
                kind.is_file() || kind.is_dir(),
                "Archive links and special files are forbidden"
            );
            let name = entry
                .path()?
                .to_str()
                .context("Invalid archive filename")?
                .to_owned();
            output.write(
                &name,
                entry.size(),
                entry.header().mode()?,
                kind.is_dir(),
                &mut entry,
            )?;
        }
    } else {
        let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let mode = entry.unix_mode().unwrap_or(0o644);
            ensure!(
                matches!(mode & 0o170_000, 0 | 0o100_000 | 0o040_000),
                "Archive links and special files are forbidden"
            );
            let name = entry.name().to_owned();
            output.write(&name, entry.size(), mode, entry.is_dir(), &mut entry)?;
        }
    }
    let files = output.files;
    validate_inventory(destination, target, &files)?;
    Ok(StagedUpdate {
        root: destination.to_owned(),
        files,
        target,
    })
}

fn validate_inventory(destination: &Path, target: Target, files: &[PathBuf]) -> Result<()> {
    #[cfg(not(unix))]
    let _ = destination;
    let executable = match target {
        Target::WindowsX86_64 => "picora.exe",
        Target::LinuxX86_64 => "picora",
        Target::MacosAarch64 => "Picora.app/Contents/MacOS/picora",
    };
    ensure!(
        files.contains(&PathBuf::from(executable)),
        "Archive is missing the application executable"
    );
    if target == Target::MacosAarch64 {
        ensure!(
            files.contains(&PathBuf::from("Picora.app/Contents/Info.plist")),
            "Archive is missing its bundle metadata"
        );
    }
    #[cfg(unix)]
    if target != Target::WindowsX86_64 {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            fs::metadata(destination.join(executable))?
                .permissions()
                .mode()
                & 0o111
                != 0,
            "Application is not executable"
        );
    }
    Ok(())
}

struct Extraction<'a> {
    destination: &'a Path,
    seen: HashSet<String>,
    files: Vec<PathBuf>,
    total: u64,
    entries: usize,
}
impl Extraction<'_> {
    fn write(
        &mut self,
        name: &str,
        size: u64,
        mode: u32,
        directory: bool,
        reader: &mut dyn Read,
    ) -> Result<()> {
        self.entries += 1;
        ensure!(self.entries <= 10_000, "Too many archive entries");
        let name = name.trim_end_matches('/');
        safe_relative(name)?;
        ensure!(
            name == "Picora" || name.starts_with("Picora/"),
            "Unexpected archive root"
        );
        if name == "Picora" {
            ensure!(directory, "Invalid archive root");
            return Ok(());
        }
        let relative = safe_relative(&name[7..])?;
        let key = relative.to_string_lossy().to_lowercase();
        ensure!(self.seen.insert(key), "Duplicate archive path");
        let output = self.destination.join(&relative);
        if directory {
            fs::create_dir_all(&output)?;
            return Ok(());
        }
        self.total = self
            .total
            .checked_add(size)
            .context("Archive size overflow")?;
        ensure!(self.total <= MAX_EXPANDED, "Expanded archive too large");
        fs::create_dir_all(output.parent().context("Invalid parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        let count = std::io::copy(&mut reader.take(size + 1), &mut file)?;
        ensure!(count == size, "Archive entry size mismatch");
        file.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &output,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        self.files.push(relative);
        Ok(())
    }
}
