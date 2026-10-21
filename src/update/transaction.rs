use super::{
    archive::{StagedUpdate, safe_relative},
    release::Target,
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallLayout {
    pub root: PathBuf,
    pub target: Target,
}
impl InstallLayout {
    /// # Errors
    /// Rejects unsupported platforms, missing executables, and invalid bundle layouts.
    pub fn detect(executable: &Path) -> Result<Self> {
        let target = Target::current().context("No update build for this architecture")?;
        let exe = executable.canonicalize()?;
        let root = if target == Target::MacosAarch64 {
            let root = exe
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .context("Not a macOS app bundle")?;
            ensure!(
                root.extension().is_some_and(|x| x == "app"),
                "Run Picora from its installed app bundle"
            );
            root
        } else {
            exe.parent().context("Executable has no parent")?
        };
        Self::for_root(root, target)
    }
    /// # Errors
    /// Rejects missing or symlinked roots and filesystem canonicalization failures.
    pub fn for_root(root: &Path, target: Target) -> Result<Self> {
        ensure!(
            !fs::symlink_metadata(root)?.file_type().is_symlink(),
            "Installation cannot be a symlink"
        );
        Ok(Self {
            root: root.canonicalize()?,
            target,
        })
    }
    #[must_use]
    pub fn executable(&self) -> PathBuf {
        self.root.join(match self.target {
            Target::WindowsX86_64 => "picora.exe",
            Target::LinuxX86_64 => "picora",
            Target::MacosAarch64 => "Contents/MacOS/picora",
        })
    }
    /// # Errors
    /// Rejects an installation root without a filename or parent directory.
    pub fn state_dir(&self) -> Result<PathBuf> {
        let name = self
            .root
            .file_name()
            .context("Invalid installation root")?
            .to_string_lossy();
        Ok(self
            .root
            .parent()
            .context("Invalid installation parent")?
            .join(format!(".{name}.picora-update")))
    }
    /// # Errors
    /// Rejects symlinked or unwritable installation directories and read-only Windows executables.
    pub fn check_writable(&self) -> Result<()> {
        let directory = if self.target == Target::MacosAarch64 {
            self.root.parent().context("Invalid bundle parent")?
        } else {
            &self.root
        };
        reject_symlink(directory)?;
        tempfile::NamedTempFile::new_in(directory).context(
            "Installation is not writable; move Picora to a writable directory before updating",
        )?;
        #[cfg(windows)]
        ensure!(
            !fs::metadata(self.executable())?.permissions().readonly(),
            "Picora executable is read-only"
        );
        Ok(())
    }
    /// # Errors
    /// Returns an error if the protected lock directory cannot be created or another process holds the startup gate.
    pub fn startup_gate(&self) -> Result<File> {
        let state = self.state_dir()?;
        reject_symlink(&state)?;
        fs::create_dir_all(&state)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
        }
        let path = state.join("startup.lock");
        reject_symlink(&path)?;
        let gate = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        gate.try_lock_exclusive()
            .context("Picora is being updated; try again when the update finishes")?;
        Ok(gate)
    }
    fn lock(&self) -> Result<File> {
        let dir = self.state_dir()?;
        if let Ok(m) = fs::symlink_metadata(&dir) {
            ensure!(
                m.is_dir() && !m.file_type().is_symlink(),
                "Invalid update directory"
            );
        }
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        let p = dir.join("lock");
        reject_symlink(&p)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(p)?;
        file.try_lock_exclusive()
            .context("An update is already running")?;
        Ok(file)
    }
}
#[derive(Serialize, Deserialize)]
struct Entry {
    relative: PathBuf,
    had_old: bool,
    has_new: bool,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    root: PathBuf,
    target: Target,
    entries: Vec<Entry>,
}
pub struct Transaction {
    layout: InstallLayout,
    journal: Journal,
    _lock: File,
}
fn reject_symlink(path: &Path) -> Result<()> {
    for component in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(component) {
            ensure!(!meta.file_type().is_symlink(), "Symlink in update path");
        }
    }
    Ok(())
}
fn remove(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    reject_symlink(path)?;
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    reject_symlink(source)?;
    reject_symlink(target)?;
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
        #[cfg(unix)]
        File::open(target)?.sync_all()?;
    } else {
        ensure!(source.is_file(), "Not a regular update file");
        fs::create_dir_all(target.parent().context("Invalid file parent")?)?;
        fs::copy(source, target)?;
        fs::OpenOptions::new()
            .write(true)
            .open(target)?
            .sync_all()?;
    }
    Ok(())
}
fn rename(from: &Path, to: &Path) -> Result<()> {
    reject_symlink(from)?;
    reject_symlink(to)?;
    fs::create_dir_all(to.parent().context("Invalid file parent")?)?;
    let start = Instant::now();
    loop {
        match move_file(from, to) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    File::open(from.parent().context("Invalid parent")?)?.sync_all()?;
                    File::open(to.parent().context("Invalid parent")?)?.sync_all()?;
                }
                return Ok(());
            }
            Err(e)
                if start.elapsed() < Duration::from_secs(10)
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                    ) =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }
}
fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    durable_write_with(path, |file| {
        file.write_all(bytes)?;
        Ok(())
    })
}
fn durable_write_with(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let parent = path.parent().context("Invalid parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    write(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

impl Transaction {
    /// # Errors
    /// Rejects invalid installation or staging paths, incomplete inventories, and failures creating the rollback journal or backup.
    pub fn prepare(layout: &InstallLayout, staged: &StagedUpdate) -> Result<Self> {
        ensure!(layout.target == staged.target, "Wrong update architecture");
        reject_symlink(&layout.root)?;
        let lock = layout.lock()?;
        let state = layout.state_dir()?;
        ensure!(
            !state.join("journal.json").exists(),
            "Previous update needs recovery"
        );
        remove(&state.join("new"))?;
        remove(&state.join("backup"))?;
        remove(&state.join("committed"))?;
        remove(&state.join("reverted"))?;
        fs::create_dir_all(state.join("new"))?;
        fs::create_dir_all(state.join("backup"))?;
        let mut entries = Vec::new();
        let staged_root = staged.root.canonicalize()?;
        if layout.target == Target::MacosAarch64 {
            copy_tree(&staged_root.join("Picora.app"), &state.join("new/bundle"))?;
            entries.push(Entry {
                relative: "bundle".into(),
                had_old: true,
                has_new: true,
            });
        } else {
            let inventory = layout.root.join(".picora-files.json");
            let previous: Vec<PathBuf> = if inventory.exists() {
                reject_symlink(&inventory)?;
                serde_json::from_slice(&fs::read(&inventory)?)?
            } else {
                Vec::new()
            };
            let mut files = staged.files.clone();
            for file in &files {
                let name = file.to_str().context("Invalid filename")?;
                safe_relative(name)?;
                ensure!(
                    !name.starts_with('.') && !name.to_lowercase().starts_with("unins"),
                    "Reserved application filename"
                );
            }
            for file in &files {
                copy_tree(&staged_root.join(file), &state.join("new").join(file))?;
            }
            durable_write(
                &state.join("new/.picora-files.json"),
                &serde_json::to_vec(&files)?,
            )?;
            files.push(".picora-files.json".into());
            let new_files = files.clone();
            for old in previous {
                let name = old.to_str().context("Invalid inventory")?;
                safe_relative(name)?;
                ensure!(
                    !name.starts_with('.') && !name.to_lowercase().starts_with("unins"),
                    "Invalid inventory entry"
                );
                if !files.contains(&old) {
                    files.push(old);
                }
            }
            for relative in files {
                reject_symlink(&layout.root.join(&relative))?;
                entries.push(Entry {
                    had_old: layout.root.join(&relative).exists(),
                    has_new: new_files.contains(&relative),
                    relative,
                });
            }
        }
        // Complete backups before persisting the journal or changing the install.
        for entry in &entries {
            if entry.had_old {
                let source = if layout.target == Target::MacosAarch64 {
                    layout.root.clone()
                } else {
                    layout.root.join(&entry.relative)
                };
                copy_tree(&source, &state.join("backup").join(&entry.relative))?;
            }
        }
        sync_tree_dirs(&state.join("new"))?;
        sync_tree_dirs(&state.join("backup"))?;
        sync_dir(&state)?;
        let journal = Journal {
            root: layout.root.clone(),
            target: layout.target,
            entries,
        };
        durable_write(&state.join("journal.json"), &serde_json::to_vec(&journal)?)?;
        Ok(Self {
            layout: layout.clone(),
            journal,
            _lock: lock,
        })
    }
    fn destination(&self, entry: &Entry) -> PathBuf {
        if self.layout.target == Target::MacosAarch64 {
            self.layout.root.clone()
        } else {
            self.layout.root.join(&entry.relative)
        }
    }
    /// # Errors
    /// Returns an error if staged files cannot be installed or transaction state cannot be recorded. Callers must roll back a failed application.
    pub fn apply(&mut self) -> Result<()> {
        let state = self.layout.state_dir()?;
        for entry in &self.journal.entries {
            let destination = self.destination(entry);
            if entry.has_new {
                replace(&state.join("new").join(&entry.relative), &destination)?;
            } else {
                remove(&destination)?;
            }
        }
        Ok(())
    }
    /// # Errors
    /// Returns an error if the previous installation cannot be restored or rollback state cannot be recorded.
    pub fn rollback(&mut self) -> Result<()> {
        let state = self.layout.state_dir()?;
        if !state.join("journal.json").exists() {
            return Ok(());
        }
        if state.join("committed").exists() || state.join("reverted").exists() {
            return self.cleanup();
        }
        for entry in self.journal.entries.iter().rev() {
            let destination = self.destination(entry);
            let backup = state.join("backup").join(&entry.relative);
            if entry.had_old {
                ensure!(backup.exists(), "Recovery backup is missing");
                let restoring = state.join("restoring").join(&entry.relative);
                remove(&restoring)?;
                copy_tree(&backup, &restoring)?;
                replace(&restoring, &destination)?;
            } else {
                remove(&destination)?;
            }
        }
        durable_write(&state.join("reverted"), b"ok")?;
        self.cleanup()
    }
    /// # Errors
    /// Returns an error if the commit marker or transaction cleanup fails.
    pub fn commit(&mut self) -> Result<()> {
        durable_write(&self.layout.state_dir()?.join("committed"), b"ok")?;
        self.cleanup()
    }
    fn cleanup(&self) -> Result<()> {
        let state = self.layout.state_dir()?;
        // Remove the journal last: failed cleanup remains recoverable.
        remove(&state.join("backup"))?;
        remove(&state.join("new"))?;
        remove(&state.join("restoring"))?;
        remove(&state.join("journal.json"))?;
        sync_dir(&state)?;
        remove(&state.join("committed"))?;
        remove(&state.join("reverted"))?;
        sync_dir(&state)?;
        Ok(())
    }
}
/// # Errors
/// Rejects corrupt recovery state and reports failures restoring an interrupted installation.
pub fn recover(layout: &InstallLayout) -> Result<()> {
    let state = layout.state_dir()?;
    if !state.join("journal.json").exists() {
        return Ok(());
    }
    let lock = layout.lock()?;
    reject_symlink(&state.join("journal.json"))?;
    let journal: Journal = serde_json::from_slice(&fs::read(state.join("journal.json"))?)?;
    ensure!(
        journal.root == layout.root && journal.target == layout.target,
        "Recovery journal belongs to another installation"
    );
    for entry in &journal.entries {
        safe_relative(entry.relative.to_str().context("Invalid journal path")?)?;
    }
    if layout.target == Target::MacosAarch64 {
        ensure!(
            journal.entries.len() == 1 && journal.entries[0].relative == Path::new("bundle"),
            "Invalid bundle journal"
        );
    }
    let mut tx = Transaction {
        layout: layout.clone(),
        journal,
        _lock: lock,
    };
    if state.join("committed").exists() || state.join("reverted").exists() {
        tx.cleanup()
    } else {
        tx.rollback()
    }
}

fn replace(from: &Path, to: &Path) -> Result<()> {
    reject_symlink(from)?;
    reject_symlink(to)?;
    if from.is_dir() && to.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let a = std::ffi::CString::new(from.as_os_str().as_bytes())?;
            let b = std::ffi::CString::new(to.as_os_str().as_bytes())?;
            // SAFETY: both paths are valid NUL-terminated strings; atomic exchange
            // keeps a launchable bundle present even if power fails after the call.
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::renameatx_np(
                    libc::AT_FDCWD,
                    a.as_ptr(),
                    libc::AT_FDCWD,
                    b.as_ptr(),
                    libc::RENAME_SWAP,
                )
            };
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::renameat2(
                    libc::AT_FDCWD,
                    a.as_ptr(),
                    libc::AT_FDCWD,
                    b.as_ptr(),
                    libc::RENAME_EXCHANGE,
                )
            };
            ensure!(
                result == 0,
                "Cannot atomically replace application bundle: {}",
                std::io::Error::last_os_error()
            );
            File::open(from.parent().context("Invalid parent")?)?.sync_all()?;
            File::open(to.parent().context("Invalid parent")?)?.sync_all()?;
            remove(from)?;
            Ok(())
        }
        #[cfg(windows)]
        {
            anyhow::bail!("Directory replacement is unsupported on Windows")
        }
    } else {
        rename(from, to)
    }
}

#[cfg(not(windows))]
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}
#[cfg(windows)]
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: valid NUL-terminated paths, no pointers escape the synchronous call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(windows)]
    let _ = path; // Windows moves below use MOVEFILE_WRITE_THROUGH.
    Ok(())
}
fn sync_tree_dirs(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_tree_dirs(&entry.path())?;
        }
    }
    sync_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_write_never_publishes_a_partial_journal() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.json");
        assert!(
            durable_write_with(&journal, |f| {
                f.write_all(b"{partial")?;
                anyhow::bail!("simulated full disk")
            })
            .is_err()
        );
        assert!(!journal.exists());
        durable_write(&journal, b"complete").unwrap();
        assert!(durable_write(&journal, b"overwrite").is_err());
        assert_eq!(fs::read(journal).unwrap(), b"complete");
    }
}
