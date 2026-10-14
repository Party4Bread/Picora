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
}
impl Instance {
    pub fn acquire(root: &Path, library: bool, tx: Sender<bool>) -> Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
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
        let socket = root.join("picora.sock");
        let namespace = format!(
            "picora-{}",
            &hex::encode(Sha256::digest(root.to_string_lossy().as_bytes()))[..24]
        );
        let name = if cfg!(windows) {
            namespace.to_ns_name::<GenericNamespaced>()?
        } else {
            socket.to_fs_name::<GenericFilePath>()?
        };
        if lock.try_lock_exclusive().is_err() {
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
            return Err(last.unwrap().into());
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
        Ok(Some(Self { _lock: lock }))
    }
}
