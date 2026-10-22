#[cfg(not(target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
/// # Errors
/// Rejects paths the startup format cannot represent and reports registration, update, or removal failures.
pub fn set(enabled: bool, exe: &Path) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        let dirs = directories::BaseDirs::new().context("Home directory unavailable")?;
        #[cfg(target_os = "linux")]
        let file = dirs.config_dir().join("autostart/picora.desktop");
        #[cfg(target_os = "macos")]
        let file = dirs
            .home_dir()
            .join("Library/LaunchAgents/app.picora.desktop.plist");
        write_registration(enabled, exe, &file)?;
    }
    #[cfg(target_os = "windows")]
    {
        let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let status = if enabled {
            std::process::Command::new("reg")
                .args([
                    "add",
                    key,
                    "/v",
                    "Picora",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\" --background", exe.display()),
                    "/f",
                ])
                .status()?
        } else {
            std::process::Command::new("reg")
                .args(["delete", key, "/v", "Picora", "/f"])
                .status()?
        };
        anyhow::ensure!(
            status.success(),
            "Cannot update launch-at-login registration"
        );
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn write_registration(enabled: bool, exe: &Path, file: &Path) -> Result<()> {
    if !enabled {
        return match std::fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        };
    }
    let path = exe
        .to_str()
        .context("Launch-at-login requires a UTF-8 application path")?;
    #[cfg(target_os = "linux")]
    let contents = {
        anyhow::ensure!(
            !path.contains('='),
            "Desktop startup does not support '=' in the application path"
        );
        let path = path
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        // Exec quoting is decoded after key-file string escaping (Desktop Entry spec §7).
        let entry = gtk::glib::KeyFile::new();
        entry.set_string("Desktop Entry", "Type", "Application");
        entry.set_string("Desktop Entry", "Name", "Picora");
        entry.set_string("Desktop Entry", "Exec", &format!("\"{path}\" --background"));
        entry.set_boolean("Desktop Entry", "Terminal", false);
        entry.to_data().to_string()
    };
    #[cfg(target_os = "macos")]
    let contents = {
        anyhow::ensure!(
            path.chars()
                .all(|c| c >= ' ' || matches!(c, '\n' | '\r' | '\t')),
            "Launch-at-login path contains an unsupported control character"
        );
        let path = path
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\r', "&#13;");
        format!(
            "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>Label</key><string>app.picora.desktop</string><key>ProgramArguments</key><array><string>{path}</string><string>--background</string></array><key>RunAtLoad</key><true/></dict></plist>"
        )
    };
    std::fs::create_dir_all(file.parent().context("Missing startup directory")?)?;
    std::fs::write(file, contents)?;
    Ok(())
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn registration_uses_isolated_destination_and_removal_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("startup/entry");
        write_registration(false, Path::new("/test/picora"), &file).unwrap();
        assert!(!file.exists());
        write_registration(true, Path::new("/test/picora"), &file).unwrap();
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("--background")
        );
        write_registration(false, Path::new("/test/picora"), &file).unwrap();
        write_registration(false, Path::new("/test/picora"), &file).unwrap();
        assert!(!file.exists());
        std::fs::write(dir.path().join("blocked"), b"file").unwrap();
        assert!(
            write_registration(
                true,
                Path::new("/test/picora"),
                &dir.path().join("blocked/entry")
            )
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_entry_preserves_quoted_path_through_both_escape_layers() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("picora.desktop");
        let key_file = gtk::glib::KeyFile::new();
        for path in [
            Path::new("/test/한글 space\\quote\"dollar$back`/picora"),
            Path::new("/test/line\ncarriage\rtab\t/picora"),
        ] {
            write_registration(true, path, &file).unwrap();
            key_file
                .load_from_file(&file, gtk::glib::KeyFileFlags::NONE)
                .unwrap();
            let command = key_file.string("Desktop Entry", "Exec").unwrap();
            let args = gtk::glib::shell_parse_argv(command.as_str()).unwrap();
            assert_eq!(
                args,
                [path.as_os_str(), std::ffi::OsStr::new("--background")]
            );
        }
        write_registration(true, Path::new("/test/100%/picora"), &file).unwrap();
        key_file
            .load_from_file(&file, gtk::glib::KeyFileFlags::NONE)
            .unwrap();
        assert!(
            key_file
                .string("Desktop Entry", "Exec")
                .unwrap()
                .contains("100%%")
        );
    }

    #[test]
    fn invalid_encoding_cannot_replace_a_working_registration() {
        use std::os::unix::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("entry");
        write_registration(true, Path::new("/test/picora"), &file).unwrap();
        let before = std::fs::read(&file).unwrap();
        let invalid = std::ffi::OsString::from_vec(vec![b'/', 0xff]);
        assert!(write_registration(true, Path::new(&invalid), &file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), before);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_escapes_xml_path_characters() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("picora.plist");
        write_registration(true, Path::new("/test/a&<b>/Picora"), &file).unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("<string>/test/a&amp;&lt;b&gt;/Picora</string>"));
    }
}
