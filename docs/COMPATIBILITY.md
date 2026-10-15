# Compatibility and verification

Status is evidence-based; a compiling adapter is not a verified desktop integration.

| Area | Status |
| --- | --- |
| Rust Linux build and tests | Linux debug/release builds and packaged tarball, 16 tests, formatting and Clippy passed on 2026-10-14 |
| DCInside Korean search, pack details, image retrieval | Live network smoke test passed on 2026-10-14 |
| Arca HTML parsing | Synthetic fixtures only; unauthenticated HTTP requests returned 403 |
| Arca login/session/catalog/download | Requires a real user login and live validation |
| Windows session persistence | Synthetic large-session DPAPI roundtrip, replacement, fresh-process reload, corruption rejection, and deletion tests; legacy Credential Manager size limit reproduced |
| Linux X11 picker/library/clipboard | Verified under Xvfb/Openbox: live catalog, pack import/restart, global shortcut, Enter selection, one Ctrl+V, valid PNG received, no Enter sent, clipboard alive while hidden, and CLI library activation |
| GNOME/KDE Wayland shortcut, clipboard and keyboard portals | Implemented, not yet exercised in a real compatible session |
| Windows native integration | Native Rust 1.90 x64 MSVC: 16 tests, fmt and Clippy passed on 2026-10-14; hidden-window regression passed for existing/new library windows; live catalog/thumbnails, preview, pack download, library copy, CLI reopen and global picker shortcut verified. See [Windows diagnosis](WINDOWS-DEBUG.md). |
| macOS native integration | Local cross-check blocked by absent Apple SDK; native CI/OS runs not yet observed |
| Discord and Telegram image/file paste | Manual target-app verification required on each OS |

## Self-updater verification (2026-10-15)

Local Linux tests cover release validation, archive boundaries, transactional replacement, rollback, and external recovery. A real Linux GUI helper smoke verified replacement, restart acknowledgement and successful cleanup. Linux and Windows cross-target Clippy passed. Native Windows/macOS helper tests run in CI; a full desktop update smoke on those systems remains separate. No live release was published for this feature; the GitHub "no newer release" path was exercised.

## Manual acceptance procedure

For each supported desktop (Windows, macOS, X11, GNOME Wayland, KDE Wayland):

1. Start the app, add a DCInside pack, restart, and use it offline.
2. In Discord and Telegram, focus a compose field. Invoke the picker, select a transparent image, and confirm one image attachment appears without sending the message.
3. Select an animated sticker with normal selection and original-file paste; record whether it inserts a still, animation, or file, and the exact app/OS versions.
4. Change focus or dismiss the picker during a download; confirm no automatic paste occurs.
5. Decline/revoke input permission; confirm the clipboard still works where a clipboard backend exists and the UI explains manual paste.
6. Verify shortcut conflicts, keyboard navigation, Korean IME, multiple displays/scaling, tray reopen, and CLI activation.
7. Sign into Arca in the helper, close it, browse/search/import a pack, restart and reuse the session. Expire it and reconnect. Sign out and confirm saved credentials are removed. Verify with and without an unlocked credential store.
8. Interrupt downloads or disconnect the network; retry without losing existing downloaded packs or group memberships.

## Known boundaries

- Arca markup/selectors and its search parameter need authenticated live confirmation. The adapter reports errors rather than silently treating unknown pages as empty results.
- Linux windows use X11/XWayland: winit 0.30 has no-op visibility and focus methods on its native Wayland backend. XWayland is therefore required on Wayland desktops. Wayland window activation/focus restoration is compositor-controlled. No universal Wayland foreground-window query exists in this app. Automatic paste depends on the compositor restoring the prior app after the picker hides.
- Wayland input permission is session-scoped and must be re-enabled after restarting Picora. Older desktops may not expose the required portals.
- File-paste acceptance and animated preservation belong to the destination app. Picora does not claim successful insertion based only on key injection.
- Animation preview is bounded to 120 frames / 64 MiB decoded memory per asset. Originals are retained intact.
- GitHub self-updates use direct file replacement; see [updates](UPDATES.md). No signing, notarization, cloud sync, or automatic cache eviction of original attachments is included.
