//! DPAPI supports full cookie jars without Credential Manager's 2,560-byte limit.
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
};

pub(super) fn path() -> Result<PathBuf> {
    let root = if let Some(path) = std::env::var_os("PICORA_DATA_DIR") {
        PathBuf::from(path)
    } else {
        directories::ProjectDirs::from("app", "Picora", "Picora")
            .context("Cannot locate session directory")?
            .data_local_dir()
            .to_owned()
    };
    Ok(root.join("arca-session.dpapi"))
}

fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: input
            .len()
            .try_into()
            .context("Session exceeds Windows data limit")?,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: Both blobs live through the synchronous call. Windows only reads
    // input; it allocates output, which is copied and freed exactly once below.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error()).context("Windows session protection failed");
    }
    // SAFETY: Successful DPAPI calls return cbData initialized bytes allocated
    // with LocalAlloc. No LOCAL_MACHINE flag: protection is per Windows user.
    let bytes = unsafe {
        let bytes = if output.cbData == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
        };
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(bytes)
}

pub(super) fn save(path: &Path, cookies: &[String]) -> Result<()> {
    let plaintext = serde_json::to_vec(cookies)?;
    ensure!(
        plaintext.len() < 1024 * 1024,
        "Session exceeds supported size"
    );
    let encrypted = crypt(&plaintext, true)?;
    let parent = path.parent().context("Missing session directory")?;
    std::fs::create_dir_all(parent).context("Cannot create session directory")?;
    // Only ciphertext reaches disk; replacement preserves the previous session
    // if encryption or writing fails, and never exposes a partially written jar.
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&encrypted)?;
    file.as_file().sync_all()?;
    file.persist(path).context("Cannot replace saved session")?;
    Ok(())
}

pub(super) fn load(path: &Path) -> Result<Option<Vec<String>>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("Cannot open saved session"),
    };
    let mut encrypted = Vec::new();
    file.take(2 * 1024 * 1024 + 1).read_to_end(&mut encrypted)?;
    ensure!(
        encrypted.len() <= 2 * 1024 * 1024,
        "Saved session exceeds supported size"
    );
    let plaintext = crypt(&encrypted, false)?;
    serde_json::from_slice(&plaintext)
        .map(Some)
        .context("Saved session is invalid")
}

pub(super) fn forget(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("Cannot delete saved session"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_session_roundtrip_replace_and_forget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.dpapi");
        let cookies = vec![format!(
            "session={}; Domain=arca.live; Secure; HttpOnly",
            "synthetic".repeat(4000)
        )];
        save(&path, &cookies).unwrap();
        let disk = std::fs::read(&path).unwrap();
        assert!(!disk.windows(9).any(|w| w == b"synthetic"));
        assert_eq!(load(&path).unwrap(), Some(cookies));
        let replacement = vec!["session=new; Domain=arca.live".to_owned()];
        save(&path, &replacement).unwrap();
        assert_eq!(load(&path).unwrap(), Some(replacement.clone()));
        assert!(save(&path, &["x".repeat(1024 * 1024)]).is_err());
        assert_eq!(load(&path).unwrap(), Some(replacement));
        forget(&path).unwrap();
        forget(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(load(&path).unwrap(), None);
    }

    #[test]
    fn corrupt_session_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.dpapi");
        save(&path, &["synthetic=session".into()]).unwrap();
        let mut disk = std::fs::read(&path).unwrap();
        let last = disk.len() - 1;
        disk[last] ^= 0xff;
        std::fs::write(&path, disk).unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn session_survives_process_restart() {
        const TEST_PATH: &str = "PICORA_SYNTHETIC_SESSION_TEST";
        let cookies = vec![format!("synthetic-session={}", "x".repeat(8000))];
        if let Some(path) = std::env::var_os(TEST_PATH) {
            assert_eq!(load(Path::new(&path)).unwrap(), Some(cookies));
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.dpapi");
        save(&path, &cookies).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "auth::windows::tests::session_survives_process_restart",
            ])
            .env(TEST_PATH, &path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "fresh process could not load saved session"
        );
    }
}
