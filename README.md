# Picora

A Rust desktop sticker picker for Windows, macOS, and Linux. Browse DCInside and Arca packs, keep favorite collections, and paste reactions through the clipboard.

## Run

### Windows installer

In GitHub **Actions → Rust desktop**, select a successful run and download the
`picora-Windows-X64` artifact. Extract it and run
`picora-<version>-windows-x64-setup.exe`. The same artifact includes the portable ZIP.
The installer installs for the current user without administrator rights, adds
Start menu shortcuts, and offers an optional desktop shortcut. Use Windows
Installed apps to uninstall; downloaded packs, settings, and encrypted sessions
are preserved. Quit Picora from its tray menu before upgrading or uninstalling.
Launch at login remains an opt-in setting inside Picora.

To build locally, install Inno Setup 6 and Python 3.11 or newer, then run:

```powershell
cargo build --release --locked
./scripts/build-installer.ps1
# Or: ./scripts/build-installer.ps1 -Compiler 'C:\path\to\ISCC.exe'
```

The installer is unsigned. WebView2 is required for Arca login, as described below.

### From source

Install Rust stable, then:

```sh
cargo run --release -- --library
```

Linux build dependencies (Debian/Ubuntu):

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libx11-dev libxrandr-dev libxi-dev libxdo-dev libgl1-mesa-dev libdbus-1-dev
```

Windows needs the WebView2 runtime for Arca login. macOS uses the system WebKit. Linux runtime needs WebKitGTK 4.1, GTK 3, AppIndicator, and X11 or XWayland. On Wayland desktops, Picora deliberately creates XWayland windows because the current winit Wayland backend cannot hide/show transient windows. Keyboard and shortcut integration still uses Wayland desktop portals.

For Windows source builds, install Visual Studio C++ Build Tools and a current
Rust MSVC toolchain (`rustup update stable`). Rust 1.90 MSVC is verified; an old
toolchain named `stable` may still need updating. Use PowerShell to isolate data:

```powershell
$env:PICORA_DATA_DIR = "$PWD\test-data"
cargo +stable run --locked -- --library
```

Picora includes a local eframe 0.33.3 backport for Windows hidden-window
handling. Keep `vendor/eframe` with the source checkout; see
[patch provenance](vendor/eframe/PICORA-PATCH.md).

## Use

1. Open **Discover**. Search DCInside, select a pack, and choose **Add pack to favorites**. You can also paste `https://dccon.dcinside.com/#41982` or an Arca `/e/<id>` URL into the search field.
2. For Arca, use **Accounts → Sign in / reconnect**. Sign in on the actual website, then close that window to return its cookies to Picora. Windows saves cookies in `arca-session.dpapi`, encrypted for your Windows account; other platforms use the OS credential store. If saving fails, the session remains in memory and the error is shown. Arca's live authenticated integration still needs verification; see the compatibility record.
3. Press **Ctrl+Shift+Space** to open the picker. Search pack/sticker titles, use arrows and Enter, or click a sticker. Picora copies the image, hides, and attempts the paste shortcut. It never presses Enter to send a message.
4. Use **Manage** below a library sticker (or right-click in the picker) to favorite/unfavorite it, remove it from the library, or add it to a custom group created in **Groups**. Individual favorite choices survive pack refreshes. Removing a sticker also removes its group membership and history; reimport its source pack to restore it.
5. Hover over a sticker to preview animation. Normal selection pastes GIF and animated WebP as files; APNG gets an animated GIF copy for chat-attachment compatibility. **Paste original file** always preserves the source bytes, including APNG. **Copy still image** explicitly copies a PNG frame. GIF conversion may reduce colors and partial transparency; use the original when the destination supports APNG.

**Recent** lists used stickers by last use, followed by unused stickers from newest imports. Reimporting an existing pack does not move its existing stickers to the front. Libraries created by older versions have no original import timestamps; their initial order is inferred from stored insertion order.

Favorites are saved as soon as you add a pack, before media downloads finish.
If you quit during a download, the favorite remains after restarting; uncached
stickers download when used, or you can reopen the pack and retry the full download.

The library window copies without auto-pasting because it is a management surface, not a transient picker. When automatic paste is unavailable, press Ctrl+V / Cmd+V in your chat app. Picora retains clipboard contents and original files after selection.

## Desktop integration

- **Windows:** configurable global shortcut and paste injection. Elevated target apps may reject injection.
- **macOS:** grant Accessibility permission to Picora (or the terminal when running an unpackaged build) for paste injection.
- **Linux X11:** global shortcut, focus restoration and X11 clipboard ownership.
- **Wayland:** approve the desktop shortcut dialog; enable keyboard access in **Settings** for automatic paste. Remote Desktop permission requests keyboard access only. Clipboard portal support varies by desktop. If the shortcut portal is unavailable, bind `picora --show` in desktop settings. Unsupported or declined permissions produce a manual-paste fallback; sessions without XWayland cannot start the picker.

Picora starts quietly in the system tray by default, with no open window. On Windows, look in the taskbar's hidden-icons arrow if the icon is not visible. Right-click the Picora icon to open the picker or library, hide both windows with **Hide to tray**, or quit. Closing a window keeps Picora running in the tray; **Ctrl+Shift+Space** reopens the picker. If tray creation reports an error, the library opens with that error instead. Settings also includes a Quit button.

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
# Windows interactive desktop: hidden picker/library/reopen regression
cargo test --locked --test windows_viewports -- --ignored --nocapture
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

## Self-updates

Settings → Updates checks GitHub Releases and offers **Update and restart**. Updates replace app files directly on all platforms, including Windows, while preserving your library and settings. See [update and recovery details](docs/UPDATES.md). An existing installation needs one manual upgrade to acquire this updater.
