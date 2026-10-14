//! Session credentials cross only an inherited pipe and the OS credential store.
use anyhow::{Context, Result, ensure};
use std::io::{Read, Write};
const SERVICE: &str = "app.picora.desktop";
const ACCOUNT: &str = "arca-session";
pub fn load() -> Result<Vec<String>> {
    let value = keyring::Entry::new(SERVICE, ACCOUNT)?.get_password()?;
    Ok(serde_json::from_str(&value)?)
}
pub fn save(cookies: &[String]) -> Result<()> {
    keyring::Entry::new(SERVICE, ACCOUNT)?.set_password(&serde_json::to_string(cookies)?)?;
    Ok(())
}
pub fn forget() -> Result<()> {
    match keyring::Entry::new(SERVICE, ACCOUNT)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub async fn login(cookies: Vec<String>) -> Result<Vec<String>> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("--auth-helper")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().context("Login pipe unavailable")?;
    stdin.write_all(&serde_json::to_vec(&cookies)?).await?;
    drop(stdin);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(900),
        child.wait_with_output(),
    )
    .await
    .context("Login timed out")??;
    ensure!(
        result.status.success(),
        "Login window could not start. Check WebView2/WebKitGTK installation."
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
        .with_title("Picora · Sign in to Arca, then close this window")
        .with_inner_size(tao::dpi::LogicalSize::new(900., 720.))
        .build(&event_loop)?;
    let builder = WebViewBuilder::new()
        .with_incognito(true)
        .with_url("https://arca.live/u/login?goto=%2Fe%2F")
        .with_navigation_handler(|raw| {
            url::Url::parse(&raw).is_ok_and(|u| {
                u.scheme() == "https"
                    && u.host_str()
                        .is_some_and(|h| h == "arca.live" || h.ends_with(".arca.live"))
            })
        });
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
