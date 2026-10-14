# Windows freeze handoff

## Report (2026-10-14)

The Windows x64 GNU release starts, but clicking **Browse repositories** leaves the window open and unresponsive. The user confirmed this is a freeze, not the window disappearing. Root cause has not been established; no fix has been applied.

## Reproduction and investigation

1. Build on Windows with `cargo run --locked` (default MSVC toolchain), preferably with a fresh `PICORA_DATA_DIR` directory.
2. In the empty library, click **Browse repositories**.
3. Compare with the **Discover** sidebar action. Capture debugger thread stacks while the UI is frozen.
4. Also compare the GNU release ZIP if the native MSVC build does not reproduce it.

The empty-state action is in `src/app/grid.rs`: it sets `Tab::Browse`, calls `open_library()`, then `browse()`. `open_library()` in `src/app/mod.rs` sends a Focus command to the library viewport, including when invoked from that same viewport. Investigate this path as well as the async request and repaint callbacks; this is a lead, not a confirmed diagnosis. Library rendering uses `show_viewport_immediate`.

`src/services.rs` dispatches repository browsing on Tokio; completion sends `Event::Catalog` and requests an egui repaint. `src/app/events.rs` consumes results. Check all thread stacks rather than assuming the request itself blocks the UI.

The Windows binary declares `windows_subsystem = "windows"`, so stderr may not be visible. For local debugging, temporarily remove that attribute or attach a debugger to capture panic output and thread stacks.

## Existing evidence

- Linux: 16 automated tests, fmt, Clippy, debug/release builds passed.
- Live DCInside Korean search, details and image retrieval passed.
- Linux Xvfb/Openbox: browsing, import/restart, hotkey picker and image paste passed.
- Windows: cross-target Clippy and GNU release compilation passed; executable DLL dependencies inspected, ZIP includes x64 WebView2Loader.dll.
- No native Windows feature testing was performed before the ZIP was delivered. The reported freeze remains unresolved.
- Arca authenticated browsing and real Wayland/macOS integrations remain unverified.

Previous Linux smoke caught a separate nested egui context lock in keyboard navigation; it was fixed and has a regression test in `src/app/grid.rs`. Do not assume that fix covers this Windows freeze.
