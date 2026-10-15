# eframe 0.33.3: Windows hidden-window backport

Source: crates.io `eframe` 0.33.3 (MIT OR Apache-2.0).
The original `src`, `data`, normalized `Cargo.toml`, and `README.md` are retained.
License texts come from the upstream 0.33.3 tag.

Applied upstream commit:
https://github.com/emilk/egui/commit/cd3c38cf2a0d65330a645faeadb6f666bc8df8f2
(PR #7905, "Improve behavior of invisible windows").

The patch changes four files under `src/native`: `run.rs`,
`winit_integration.rs`, `glow_integration.rs`, and `wgpu_integration.rs`.
It directly updates hidden/minimized windows when a repaint is due and
throttles their repaint requests. Windows does not deliver ordinary redraw
events to hidden windows; Picora hides its root picker while the immediate
library viewport is open, so the unpatched backend stops running the app.

One additional backport adjustment in `run.rs`: wrap `new_events`' call to
`check_redraw_requests` in `with_event_loop_context`. That path can now render
and must supply the active event loop when creating an immediate viewport.
Without it, opening the library after background startup panics because the
immediate viewport callback never runs.

Regression: `tests/windows_viewports.rs` runs native Windows subprocesses for
an existing library and for a library first opened after the picker is hidden.
Both verify actual native picker visibility on reopening. The original backend
times out; the unadjusted upstream backport fails the background case.

Remove this override when migrating Picora to an eframe version with the fix
and its newer App/UI lifecycle API. Re-run the native regression and the
Windows acceptance procedure before removing it.
