//! Sessions use an inherited pipe and OS-protected storage, never plaintext files.
use anyhow::{Context, Result, ensure};
use std::io::{Read, Write};
const SERVICE: &str = "app.picora.desktop";
const ACCOUNT: &str = "arca-session";
#[cfg(target_os = "windows")]
mod windows;
/// # Errors
/// Returns an error if credentials are missing, storage is unavailable, or the saved cookie jar is invalid.
pub fn load() -> Result<Vec<String>> {
    #[cfg(target_os = "windows")]
    if let Some(cookies) = windows::load(&windows::path()?)? {
        return Ok(cookies);
    }
    // Older Windows builds stored small sessions in Credential Manager.
    let value = keyring::Entry::new(SERVICE, ACCOUNT)?.get_password()?;
    Ok(serde_json::from_str(&value)?)
}
/// # Errors
/// Returns an error if serialization or OS-protected credential storage fails.
pub fn save(cookies: &[String]) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::save(&windows::path()?, cookies);
    #[cfg(not(target_os = "windows"))]
    {
        keyring::Entry::new(SERVICE, ACCOUNT)?.set_password(&serde_json::to_string(cookies)?)?;
        Ok(())
    }
}
/// # Errors
/// Returns an error if credential storage cannot be accessed or cleared; missing credentials are accepted.
pub fn forget() -> Result<()> {
    let legacy = match keyring::Entry::new(SERVICE, ACCOUNT)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    };
    #[cfg(target_os = "windows")]
    windows::forget(&windows::path()?)?;
    legacy
}
/// # Errors
/// Returns an error if the helper cannot start, times out, exits unsuccessfully, or returns an empty or invalid session.
pub async fn login(cookies: Vec<String>) -> Result<Vec<String>> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    #[cfg(target_os = "linux")]
    {
        // Match the main UI's XWayland requirement. WebKit's DMA-BUF path
        // can crash with a Wayland protocol error or render a blank login
        // window on Plasma; use its fallback renderer for this helper only.
        command.env("GDK_BACKEND", "x11");
        command.env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    let mut child = command
        .arg("--auth-helper")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().context("Login pipe unavailable")?;
    stdin.write_all(&serde_json::to_vec(&cookies)?).await?;
    drop(stdin);
    let result = tokio::time::timeout(std::time::Duration::from_mins(15), child.wait_with_output())
        .await
        .context("Login timed out")??;
    ensure!(
        result.status.success(),
        "arca.live sign-in window exited unexpectedly ({}). Its diagnostic output is available in Picora’s process log.",
        result.status
    );
    ensure!(
        result.stdout.len() < 1024 * 1024,
        "Session response too large"
    );
    let cookies: Vec<String> =
        serde_json::from_slice(&result.stdout).context("Login window returned no session")?;
    ensure!(
        !cookies.is_empty(),
        "No session cookies received. Finish signing in before closing the login window."
    );
    Ok(cookies)
}
/// # Errors
/// Returns an error if the inherited session is invalid or the native sign-in window cannot initialize.
pub fn helper() -> Result<()> {
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };
    use wry::WebViewBuilder;
    let mut input = String::new();
    std::io::stdin()
        .take(1024 * 1024)
        .read_to_string(&mut input)?;
    let cookies: Vec<String> = serde_json::from_str(&input)?;
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Picora · Sign in to arca.live, then close this window")
        .with_inner_size(tao::dpi::LogicalSize::new(900., 720.))
        .build(&event_loop)?;
    let builder = WebViewBuilder::new()
        .with_incognito(true)
        .with_url("https://arca.live/u/login?goto=%2Fe%2F")
        .with_navigation_handler(|raw| allowed_login_navigation(&raw));
    #[cfg(target_os = "linux")]
    let view = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(
            window
                .default_vbox()
                .context("Login container unavailable")?,
        )?
    };
    #[cfg(not(target_os = "linux"))]
    let view = builder.build(&window)?;
    for cookie in cookies {
        if let Ok(c) = wry::cookie::Cookie::parse(cookie) {
            let _ = view.set_cookie(&c);
        }
    }
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            let result: Vec<String> = view
                .cookies()
                .unwrap_or_default()
                .into_iter()
                .filter(|c| {
                    c.domain().is_some_and(|d| {
                        d.trim_start_matches('.') == "arca.live" || d.ends_with(".arca.live")
                    })
                })
                .map(|c| c.to_string())
                .collect();
            if let Ok(bytes) = serde_json::to_vec(&result) {
                let _ = std::io::stdout().write_all(&bytes);
                let _ = std::io::stdout().flush();
            }
            *flow = ControlFlow::Exit;
        }
    });
}

fn allowed_login_navigation(raw: &str) -> bool {
    // Turnstile uses empty/srcdoc subframes as well as its HTTPS frame.
    // https://developers.cloudflare.com/turnstile/get-started/mobile-implementation/
    if matches!(raw, "about:blank" | "about:srcdoc") {
        return true;
    }
    url::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.host_str().is_some_and(|host| {
                // WebKit applies this policy to subframes too. Arca embeds
                // its Turnstile challenge on this separate, exact host.
                host == "arca.live"
                    || host.ends_with(".arca.live")
                    || host == "challenges.cloudflare.com"
            })
    })
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn arca_login_can_load_cloudflare_challenge_frame() {
        assert!(allowed_login_navigation("https://arca.live/u/login"));
        assert!(allowed_login_navigation("about:blank"));
        assert!(allowed_login_navigation("about:srcdoc"));
        assert!(allowed_login_navigation(
            "https://challenges.cloudflare.com/turnstile/v0/api.js"
        ));
        assert!(allowed_login_navigation(
            "https://challenges.cloudflare.com/cdn-cgi/challenge-platform/h/g/turnstile/if/av0/test"
        ));
    }
    #[test]
    fn unrelated_and_lookalike_navigation_is_still_rejected() {
        for url in [
            "http://challenges.cloudflare.com/",
            "https://challenges.cloudflare.com.evil.example/",
            "https://evil.example/?next=https://arca.live/",
            "https://arca.live@evil.example/",
            "https://www.google.com/",
            "file:///etc/passwd",
            "about:config",
            "about:blank.evil",
            "javascript:alert(1)",
        ] {
            assert!(!allowed_login_navigation(url), "{url}");
        }
    }
}
