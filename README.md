# Picora

A Rust desktop sticker picker for Windows, macOS, and Linux. Browse DCInside and Arca packs, keep favorite collections, and paste reactions through the clipboard.

## Run

Install Rust stable, then:

```sh
cargo run --release -- --library
```

Linux build dependencies (Debian/Ubuntu):

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libdbus-1-dev
```

Windows needs the WebView2 runtime for Arca login. macOS uses the system WebKit. Linux runtime needs WebKitGTK 4.1, GTK 3, AppIndicator, and X11 or XWayland. On Wayland desktops, Picora deliberately creates XWayland windows because the current winit Wayland backend cannot hide/show transient windows. Keyboard and shortcut integration still uses Wayland desktop portals.

## Use

1. Open **Discover**. Search DCInside, select a pack, and choose **Add pack to favorites**. You can also paste `https://dccon.dcinside.com/#41982` or an Arca `/e/<id>` URL into the search field.
2. For Arca, use **Accounts → Sign in / reconnect**. Sign in on the actual website, then close that window to return its cookies to Picora. Credentials go to the OS credential store, or remain in memory if unavailable. Arca's live authenticated integration still needs verification; see the compatibility record.
3. Press **Ctrl+Shift+Space** to open the picker. Search pack/sticker titles, use arrows and Enter, or click a sticker. Picora copies the image, hides, and attempts the paste shortcut. It never presses Enter to send a message.
4. Create custom groups in **Groups**. Right-click a downloaded sticker to add it, change its order, or remove it from a group.
5. Hover over a sticker to preview animation. **Paste original file** offers the original GIF/WebP to apps that support file pastes. Normal selection copies a still image with transparency. Receiving apps decide which formats they accept.

The library window copies without auto-pasting because it is a management surface, not a transient picker. When automatic paste is unavailable, press Ctrl+V / Cmd+V in your chat app. Picora retains clipboard contents and original files after selection.

## Desktop integration

- **Windows:** configurable global shortcut and paste injection. Elevated target apps may reject injection.
- **macOS:** grant Accessibility permission to Picora (or the terminal when running an unpackaged build) for paste injection.
- **Linux X11:** global shortcut, focus restoration and X11 clipboard ownership.
- **Wayland:** approve the desktop shortcut dialog; enable keyboard access in **Settings** for automatic paste. Remote Desktop permission requests keyboard access only. Clipboard portal support varies by desktop. If the shortcut portal is unavailable, bind `picora --show` in desktop settings. Unsupported or declined permissions produce a manual-paste fallback; sessions without XWayland cannot start the picker.

Tray menu: open picker, open library, quit. Closing a window keeps the background process alive. Settings includes a Quit button for desktops without tray support.

```sh
picora --show
picora --library
picora --background
```

Only one instance runs per data directory. `PICORA_DATA_DIR=/path` overrides the default OS app-data location for development or isolated testing. SQLite metadata and downloaded images remain local. Removing a pack removes library/group records but retains cached files for clipboard safety. Launch at login is opt-in.

## Develop and verify

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
# Explicit network smoke test, excluded from the normal suite:
cargo test --test live -- --ignored
```

Tests include real DCInside response fixtures and **synthetic** Arca fixtures; synthetic fixtures are not evidence of authenticated Arca compatibility. No credentials are required by CI. See [compatibility and validation](docs/COMPATIBILITY.md) for what has actually been tested.

Modules separate models/SQLite, repository adapters, media/cache, background services, native platform integration, login helper, and egui UI. The Wry helper runs as a separate process to isolate its event loop and website content from the Rust UI.

## Distribution

```sh
cargo build --release
python scripts/package.py
```

Produces unsigned archives in `dist/`: Windows executable, macOS `.app`, or Linux binary plus desktop entry/icon. CI builds and tests on all three OS runners and uploads archives; it does not publish releases or sign/notarize applications. Linux archives require the system libraries above. Install the Linux desktop entry to `~/.local/share/applications/` and icon to `~/.local/share/icons/hicolor/scalable/apps/`, with the binary on PATH.

Noto Sans KR is bundled under the [SIL Open Font License](assets/OFL.txt), sourced from [Google Fonts](https://github.com/notofonts/noto-cjk/tree/main/Sans/SubsetOTF/KR). Stickers are downloaded from their original repositories at the user's request; no sticker artwork is bundled.
