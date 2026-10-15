#[cfg(not(target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
pub fn set(enabled: bool, exe: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let dirs = directories::BaseDirs::new().context("Home directory unavailable")?;
        let dir = dirs.config_dir().join("autostart");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("picora.desktop");
        if enabled {
            let path = exe
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            std::fs::write(
                file,
                format!(
                    "[Desktop Entry]\nType=Application\nName=Picora\nExec=\"{path}\" --background\nTerminal=false\n"
                ),
            )?;
        } else if file.exists() {
            std::fs::remove_file(file)?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = directories::BaseDirs::new().context("Home unavailable")?;
        let dir = home.home_dir().join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("app.picora.desktop.plist");
        if enabled {
            let path = exe
                .to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            std::fs::write(
                file,
                format!(
                    "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>Label</key><string>app.picora.desktop</string><key>ProgramArguments</key><array><string>{path}</string><string>--background</string></array><key>RunAtLoad</key><true/></dict></plist>"
                ),
            )?;
        } else if file.exists() {
            std::fs::remove_file(file)?;
        }
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
