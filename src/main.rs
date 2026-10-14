#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use anyhow::{Context, Result};
use std::sync::mpsc;
fn main() {
    if let Err(error) = run() {
        eprintln!("Picora: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--auth-helper") {
        return picora::auth::helper();
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Picora — Rust sticker picker\n\n  --show        Open the picker\n  --library     Open the library\n  --background  Start hidden\n  --help        Show this help\n\nDefault shortcut: Ctrl+Shift+Space"
        );
        return Ok(());
    }
    anyhow::ensure!(
        args.iter()
            .all(|a| ["--show", "--library", "--background"].contains(&a.as_str())),
        "Unknown argument; use --help"
    );
    let root = if let Some(p) = std::env::var_os("PICORA_DATA_DIR") {
        std::path::PathBuf::from(p)
    } else {
        directories::ProjectDirs::from("app", "Picora", "Picora")
            .context("Cannot locate app data directory")?
            .data_local_dir()
            .to_owned()
    };
    let library = args.iter().any(|a| a == "--library");
    let background = args.iter().any(|a| a == "--background");
    let (tx, rx) = mpsc::channel();
    let Some(_instance) = picora::platform::instance::Instance::acquire(&root, library, tx)? else {
        return Ok(());
    };
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
