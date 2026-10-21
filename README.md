# Picora — Desktop Sticker Picker

**English** · [Korean](README-ko.md)

**A desktop sticker picker for Windows, macOS, and Linux, built in Rust.** Search sticker packs, save your favorites, and paste image reactions into apps that accept clipboard images or attachments.

Like the Windows emoji panel, Picora opens with a shortcut: **Ctrl+Shift+Space**. Choose a sticker to copy it and attempt to paste into your previous app. Picora never presses Enter to send a message.

**[Download the latest release](https://github.com/Party4Bread/Picora/releases/latest)** · **[Report a bug](https://github.com/Party4Bread/Picora/issues)** · **[Compatibility](docs/COMPATIBILITY.md)**

## Features

- **Search sticker packs:** browse supported sticker repositories or import a pack by URL.
- **Organize your library:** favorite individual stickers, create custom groups, and manage stickers with a right-click.
- **Find recent stickers:** recently used stickers appear first, followed by newly added ones.
- **Use animated reactions:** hover to preview; copy a still image or paste an animated file. Downloaded stickers remain available offline.
- **Work from the keyboard:** configurable global shortcut and arrow-key navigation, with light and dark themes.
- **Update in the app:** install GitHub releases from **Settings → Updates** while preserving your library.

## Download and install

| Platform | Release asset | Install |
| --- | --- | --- |
| Windows x64 | `picora-<version>-windows-x64-setup.exe` | Run the installer. A portable ZIP is also available. |
| macOS, Apple Silicon | `picora-<version>-macos-aarch64.dmg` | Drag Picora to Applications. |
| Linux x64 | `picora-<version>-linux-x86_64.tar.gz` | Extract and run `Picora/picora --library`. |

Download these files from [GitHub Releases](https://github.com/Party4Bread/Picora/releases/latest).

- **Windows:** the installer is unsigned. Website sign-in requires WebView2. Quit Picora before a manual upgrade or uninstall; uninstalling preserves your library and settings.
- **macOS:** builds are ad-hoc signed, not Apple-notarized. Read the [macOS installation guide](docs/MACOS-INSTALL.md) if Gatekeeper blocks launch. Intel Mac builds are not currently provided.
- **Linux:** install the system libraries listed under [building from source](#build-from-source). Picker windows require X11 or XWayland.

## Quick start

1. Open **Library → Discover** from the tray/menu-bar icon. Search for a pack or paste its repository URL, then choose **Add pack to favorites**.
2. Create collections in **Groups**. Right-click a sticker to favorite it, assign groups, or remove it from your library.
3. Focus your chat or editor, press **Ctrl+Shift+Space**, and choose a sticker. If automatic paste is unavailable, use **Ctrl+V** or **Cmd+V**.

Closing a window keeps Picora running in the tray. Use **Quit** to exit. The library window copies stickers without automatic paste; launch at login is opt-in.

### Repository accounts

Open **Accounts → Sign in / reconnect**, sign in on the repository’s website, complete any browser challenge, and close the login window. **Check connection** verifies catalog access. Saved sessions and connection errors appear in Accounts.

Picora stores session cookies using OS-protected storage. If saving fails, the session remains available until Picora quits. Some repositories support browsing without sign-in.

### Animated stickers

Normal selection pastes animated GIF/WebP as files; supported APNG and MP4 stickers can be converted to GIF. **Paste original file** preserves source bytes; **Copy still image** copies a PNG frame. The receiving app determines whether animations play or appear as attachments.

## Desktop integration

| Desktop | Setup |
| --- | --- |
| Windows | Global shortcut and automatic paste; elevated apps may require manual paste. |
| macOS | Grant Picora Accessibility permission for automatic paste. |
| Linux X11 | Keep Picora running to retain clipboard ownership. |
| KDE Plasma / Wayland | Approve the shortcut dialog; enable **Settings → Wayland keyboard access** for automatic paste. Native clipboard copying does not require keyboard permission. |
| Other Wayland desktops | Portal and clipboard support vary. If the shortcut portal is unavailable, bind `picora --show` in desktop settings. |

See [compatibility notes](docs/COMPATIBILITY.md) for clipboard formats, input methods, and platform limitations.

## Build from source

Install a current Rust stable toolchain, clone the repository, and run:

```sh
git clone https://github.com/Party4Bread/Picora.git
cd Picora
cargo run --release --locked -- --library
```

On Debian/Ubuntu, install build dependencies first:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libx11-dev libxrandr-dev libxi-dev libxdo-dev libgl1-mesa-dev libdbus-1-dev
```

Linux releases require GTK 3, WebKitGTK 4.1, AppIndicator, and the associated X11 libraries at runtime. macOS builds require Apple's development tools and use system WebKit. Windows builds require Visual Studio C++ Build Tools and a current Rust MSVC toolchain (`rustup update stable`).

Keep `vendor/eframe` in the checkout: it contains a Windows hidden-window fix. See [patch provenance](vendor/eframe/PICORA-PATCH.md).

### Command-line launch and local data

```sh
picora --show        # Open the picker
picora --library     # Open the library
picora --background  # Start in the tray
```

Only one instance runs per data directory. Set `PICORA_DATA_DIR` to use isolated development data:

```sh
PICORA_DATA_DIR=./test-data cargo run --locked -- --library
```

In PowerShell:

```powershell
$env:PICORA_DATA_DIR = "$PWD\test-data"
cargo run --locked -- --library
```

### Tests and packaging

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
python scripts/package.py
```

Packaging creates platform archives in `dist/`, plus a macOS DMG on macOS. On Windows, install Inno Setup 6 and Python 3.11+ to build the installer:

```powershell
./scripts/build-installer.ps1
```

Desktop and live-network checks are opt-in:

```sh
cargo test --locked --test arca_live -- --ignored --nocapture
cargo test --locked --test live live_dcinside_search_pack_and_image -- --ignored
cargo test --locked --test windows_viewports -- --ignored --nocapture
python scripts/smoke_wayland_clipboard.py
```

The Arca check uses your saved session. Windows viewport tests need an interactive desktop; the Wayland script requires headless KWin. For macOS browser clipboard checks, build `cargo build --locked --example clipboard_fixture` and follow the instructions in [smoke_macos_chrome.py](scripts/smoke_macos_chrome.py).

Packaging and installer scripts are used by CI. `test_update_manifest.py` validates release metadata; `tests/update_process.rs` runs updater replacement and recovery scenarios as part of `cargo test`. Platform smoke scripts cover desktop behavior that headless CI cannot exercise. See [release and recovery documentation](docs/UPDATES.md).

On Linux, optional desktop integration files are included in the archive. Put the binary on `PATH`, install the desktop entry to `~/.local/share/applications/`, and the icon to `~/.local/share/icons/hicolor/scalable/apps/`.

## Project

Picora uses Rust and egui. Repository adapters, SQLite storage, media caching, and native desktop integration are separate modules; website sign-in runs in a Wry helper process.

[Noto Sans KR](https://github.com/notofonts/noto-cjk/tree/main/Sans/SubsetOTF/KR) is bundled under the [SIL Open Font License](assets/OFL.txt). Sticker artwork is downloaded from its source repository at your request. Picora is not affiliated with the supported repositories.
