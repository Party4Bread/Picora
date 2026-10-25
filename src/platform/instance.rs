use anyhow::{Context, Result};
use fs2::FileExt;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Stream, prelude::*,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::mpsc::Sender,
};

pub struct Instance {
    _lock: File,
    #[cfg(target_os = "windows")]
    _installer_guard: InstallerGuard,
}

// Inno Setup checks this named mutex before installation and uninstallation.
// Holding a handle is sufficient: this does not replace the per-data-dir lock.
#[cfg(target_os = "windows")]
pub(crate) struct InstallerGuard(windows_sys::Win32::Foundation::HANDLE);
#[cfg(target_os = "windows")]
impl InstallerGuard {
    pub(crate) fn new() -> Result<Self> {
        let name: Vec<u16> = "Local\\Picora.Desktop"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: name is NUL-terminated and lives through this synchronous call.
        let handle = unsafe {
            windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(), 0, name.as_ptr())
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error()).context("Cannot create installer guard");
        }
        Ok(Self(handle))
    }
}
#[cfg(target_os = "windows")]
impl Drop for InstallerGuard {
    fn drop(&mut self) {
        // SAFETY: this struct owns the non-null handle returned by CreateMutexW.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}
impl Instance {
    /// # Errors
    /// Returns an error if the instance lock or activation socket is unavailable, or an existing instance cannot be notified.
    pub fn acquire(
        root: &Path,
        library: bool,
        background: bool,
        tx: Sender<bool>,
    ) -> Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
        #[cfg(unix)]
        let canonical_root = root.canonicalize()?;
        #[cfg(unix)]
        let root = canonical_root.as_path();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("instance.lock"))?;

        let namespace = format!(
            "picora-{}",
            &hex::encode(Sha256::digest(root.to_string_lossy().as_bytes()))[..24]
        );
        let socket = root.join("picora.sock");
        #[cfg(unix)]
        let socket = {
            use std::os::unix::{
                ffi::OsStrExt,
                fs::{DirBuilderExt, MetadataExt},
            };
            if socket.as_os_str().as_bytes().len() >= 100 {
                // macOS sockaddr_un has only 104 bytes, including its terminator.
                let uid = unsafe { libc::geteuid() };
                let directory = std::path::PathBuf::from(format!("/tmp/{namespace}-{uid}"));
                match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
                    Ok(()) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => {
                        return Err(error)
                            .context("Cannot create short activation socket directory");
                    }
                }
                let metadata = std::fs::symlink_metadata(&directory)?;
                anyhow::ensure!(
                    metadata.is_dir() && metadata.uid() == uid && metadata.mode() & 0o077 == 0,
                    "Activation socket directory is not private to this user"
                );
                directory.join("socket")
            } else {
                socket
            }
        };
        let name = if cfg!(windows) {
            namespace.to_ns_name::<GenericNamespaced>()?
        } else {
            socket.to_fs_name::<GenericFilePath>()?
        };
        if let Err(error) = lock.try_lock_exclusive() {
            if error.raw_os_error() != fs2::lock_contended_error().raw_os_error() {
                return Err(error).with_context(|| {
                    format!("Cannot lock {}", root.join("instance.lock").display())
                });
            }
            if background {
                return Ok(None);
            }
            let mut last = None;
            for _ in 0..20 {
                match Stream::connect(name.clone()) {
                    Ok(mut s) => {
                        s.write_all(if library { b"L" } else { b"S" })?;
                        return Ok(None);
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
            return Err(last
                .context("Activation connection was not attempted")?
                .into());
        }
        let listener = ListenerOptions::new()
            .name(name)
            .try_overwrite(true)
            .create_sync()
            .context("Cannot create Picora activation socket")?;
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let mut b = [0];
                    if stream.read_exact(&mut b).is_ok() && matches!(b[0], b'L' | b'S') {
                        let _ = tx.send(b[0] == b'L');
                    }
                });
            }
        });
        Ok(Some(Self {
            _lock: lock,
            #[cfg(target_os = "windows")]
            _installer_guard: InstallerGuard::new()?,
        }))
    }
}
