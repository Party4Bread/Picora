//! Native regression for Picora's hidden picker + immediate library viewport.
//! Run on an interactive Windows desktop:
//! cargo test --locked --test `windows_viewports` -- --ignored --nocapture
#![cfg(target_os = "windows")]

use eframe::egui;
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[test]
#[ignore = "opens native windows; requires an interactive Windows desktop"]
fn hidden_picker_keeps_library_alive_and_can_reopen() {
    const CHILD: &str = "PICORA_VIEWPORT_TEST_CHILD";
    if let Ok(mode) = std::env::var(CHILD) {
        run_probe(mode == "background");
        return;
    }
    for mode in ["library", "background"] {
        // A stalled event loop must fail, rather than hang the test runner forever.
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "hidden_picker_keeps_library_alive_and_can_reopen",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD, mode)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "native viewport probe failed: {status}");
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "hidden picker stalled the UI: library did not update and picker did not reopen"
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn run_probe(background: bool) {
    let completed = Arc::new(AtomicBool::new(false));
    let result = completed.clone();
    eframe::run_native(
        "Picora viewport regression",
        eframe::NativeOptions {
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            viewport: egui::ViewportBuilder::default().with_inner_size([200., 120.]),
            ..Default::default()
        },
        Box::new(move |_| {
            Ok(Box::new(Probe {
                hidden_at: None,
                reopening: false,
                background,
                completed: result,
            }))
        }),
    )
    .unwrap();
    assert!(
        completed.load(Ordering::SeqCst),
        "probe closed before completing hide/update/reopen"
    );
}

struct Probe {
    hidden_at: Option<Instant>,
    reopening: bool,
    background: bool,
    completed: Arc<AtomicBool>,
}

impl eframe::App for Probe {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let library = egui::ViewportId::from_hash_of("library");
        let RawWindowHandle::Win32(handle) = frame.window_handle().unwrap().as_raw() else {
            panic!("expected a Windows window");
        };
        // Observe native visibility, not just the queued egui command.
        let visible = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(handle.hwnd.get() as _)
                != 0
        };
        if self.reopening && visible {
            self.completed.store(true, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let hidden_long_enough = self
            .hidden_at
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(2));
        if !self.background || hidden_long_enough {
            ctx.show_viewport_immediate(
                library,
                egui::ViewportBuilder::default()
                    .with_title("Picora library regression")
                    .with_inner_size([240., 120.]),
                |ctx, _| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label(if hidden_long_enough {
                            "Library updated while picker hidden"
                        } else {
                            "Waiting for hidden picker"
                        });
                    });
                },
            );
        }
        if hidden_long_enough {
            assert!(
                !visible,
                "picker must really be hidden while library updates"
            );
            println!("Library updated while picker was hidden; reopening picker");
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.reopening = true;
        } else {
            self.hidden_at.get_or_insert_with(Instant::now);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
