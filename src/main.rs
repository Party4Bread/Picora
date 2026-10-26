#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use anyhow::{Context, Result};
use std::sync::mpsc;
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Picora: {error:#}");
            append_crash_log(&format!("Picora: {error:#}\n"));
            std::process::ExitCode::FAILURE
        }
    }
}
/// Returns `(library, background)`. Launchers pass `--library`, and autostart
/// entries pass `--background`. A macOS app bundle cannot pass arguments, so a
/// plain launch there opens the library instead of starting hidden.
fn startup_mode(args: &[String]) -> (bool, bool) {
    let library =
        args.iter().any(|a| a == "--library") || (cfg!(target_os = "macos") && args.is_empty());
    let background = !library && !args.iter().any(|a| a == "--show");
    (library, background)
}

static CRASH_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Windows release builds have no console, so panics and fatal errors are
/// also appended to `crash.log` in the data folder.
fn log_panics(root: &std::path::Path) {
    let _ = CRASH_LOG.set(root.join("crash.log"));
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        append_crash_log(&format!(
            "thread {}\n{info}\n{}\n",
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture(),
        ));
        default(info);
    }));
}
fn append_crash_log(entry: &str) {
    use std::io::Write;
    let Some(path) = CRASH_LOG.get() else {
        return;
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Start over rather than grow without bound.
    let append = std::fs::metadata(path).is_ok_and(|m| m.len() < 1024 * 1024);
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(path));
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
    {
        let _ = write!(
            file,
            "--- unix time {seconds} · Picora {}\n{entry}",
            env!("CARGO_PKG_VERSION")
        );
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--update-helper") {
        anyhow::ensure!(args.len() == 2, "Invalid updater arguments");
        return picora::update::helper::run_helper(std::path::Path::new(&args[1]));
    }
    if args.iter().any(|a| a == "--auth-helper") {
        return picora::auth::helper();
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Picora — Rust sticker picker\n\nStarts in the system tray by default (on macOS, opens the library).\n\n  --show        Open the picker\n  --library     Open the library\n  --background  Start hidden\n  --help        Show this help\n\nDefault shortcut: Ctrl+Shift+Space"
        );
        return Ok(());
    }
    anyhow::ensure!(
        args.iter()
            .all(|a| ["--show", "--library", "--background"].contains(&a.as_str())),
        "Unknown argument; use --help"
    );
    let startup = picora::update::helper::startup_recovery()?;
    if startup.restarting {
        return Ok(());
    }
    let root = if let Some(p) = std::env::var_os("PICORA_DATA_DIR") {
        std::path::PathBuf::from(p)
    } else {
        directories::ProjectDirs::from("app", "Picora", "Picora")
            .context("Cannot locate app data directory")?
            .data_local_dir()
            .to_owned()
    };
    log_panics(&root);
    let (library, background) = startup_mode(&args);
    let (tx, rx) = mpsc::channel();
    let Some(_instance) =
        picora::platform::instance::Instance::acquire(&root, library, background, tx)?
    else {
        return Ok(());
    };
    drop(startup);
    #[cfg(target_os = "linux")]
    anyhow::ensure!(
        std::env::var_os("DISPLAY").is_some(),
        "Picora requires X11 or XWayland for its transient picker window. Enable XWayland in your desktop session."
    );
    #[cfg(target_os = "linux")]
    gtk::init().context("Cannot connect to a desktop display")?;
    let runtime = tokio::runtime::Runtime::new()?;
    let handle = runtime.handle().clone();
    let options = eframe::NativeOptions {
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|builder| {
            // Winit Wayland set_visible/focus_window are no-ops. A transient picker
            // therefore needs X11/XWayland; portals still serve Wayland input.
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        })),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Picora")
            .with_app_id("app.picora.desktop")
            .with_inner_size([460., 540.])
            .with_min_inner_size([340., 320.])
            .with_visible(!background)
            .with_window_level(eframe::egui::WindowLevel::AlwaysOnTop),
        ..Default::default()
    };
    eframe::run_native(
        "Picora",
        options,
        Box::new(move |cc| {
            Ok(Box::new(picora::app::App::new(
                cc, handle, root, rx, background, library,
            )?))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Desktop UI failed: {e}"))?;
    drop(runtime);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::startup_mode;
    #[test]
    fn plain_launch_opens_library_only_where_launchers_cannot_pass_arguments() {
        let mode =
            |args: &[&str]| startup_mode(&args.iter().map(ToString::to_string).collect::<Vec<_>>());
        let macos = cfg!(target_os = "macos");
        assert_eq!(mode(&[]), (macos, !macos));
        assert_eq!(mode(&["--background"]), (false, true));
        assert_eq!(mode(&["--library"]), (true, false));
        assert_eq!(mode(&["--show"]), (false, false));
    }
}
