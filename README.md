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

## Supported sticker repositories

These are Picora’s currently supported sources. Your stickers are organized together in one library.

| Repository | Browse and import | Sign-in |
| --- | --- | --- |
| [DCInside](https://dccon.dcinside.com/) | Search packs or import by pack URL | Not required |
| [arca.live (아카라이브)](https://arca.live/e/) | Search packs or import by pack URL | Required; website verification may be requested |
| [GIPHY](https://giphy.com/) | Individual GIFs and stickers; copy, favorite, or add each one to a group | Your own GIPHY API key |

GIPHY: open **Library → Accounts**, enter your API key, and choose **Use and save key**. Keys use OS-protected storage. Then choose **GIPHY** in **Discover** to search stickers or GIFs (an empty search shows trending). Click a result to copy it; right-click to favorite it or add it to a group. Kept items are saved in the **Saved from GIPHY** collection and work in the picker, Recent, and groups like any other sticker. Request limits depend on your API plan. The bundled attribution marks come from [GIPHY's official attribution kit](https://media.giphy.com/giphy-attribution-marks.zip).

## Download and install

| Platform | Release asset | Install |
| --- | --- | --- |
| Windows x64 | `picora-<version>-windows-x64-setup.exe` | Run the installer. A portable ZIP is also available. |
| macOS, Apple Silicon | `picora-<version>-macos-aarch64.dmg` | Drag Picora to Applications. Opening it shows the library. |
| Linux x64 | `picora-<version>-linux-x86_64.tar.gz` | Extract and run `Picora/picora --library`. |

Download these files from [GitHub Releases](https://github.com/Party4Bread/Picora/releases/latest).

Choose **Settings → Updates → Update channel → Dev** to receive the newest main build that passed all platform CI checks. Dev builds may contain unfinished changes. Updates still require **Update and restart**; choose **Stable** to return to the latest stable release. Documentation-only commits do not produce a new build.

- **Windows:** the installer is unsigned. Website sign-in requires WebView2. Quit Picora before a manual upgrade or uninstall; uninstalling preserves your library and settings.
- **macOS:** builds are ad-hoc signed, not Apple-notarized. Read the [macOS installation guide](docs/MACOS-INSTALL.md) if Gatekeeper blocks launch. Intel Mac builds are not currently provided.
- **Linux:** install the system libraries listed under [building from source](#build-from-source). Picker windows require X11 or XWayland.

## Quick start

1. Open **Library → Discover** from the tray/menu-bar icon. Search for a pack or paste its repository URL, then choose **Download pack** to keep it offline, or **Favorite pack** to add it to Favorites.
2. Create a group with **+** next to **Custom groups** in the library sidebar; right-click a group to rename or delete it. Click the star on a sticker to favorite it. Right-click a sticker to assign groups or remove it from your library. In a pack preview or GIPHY results, right-click adds just that sticker without the rest of the pack. Turn on **Select** (or Ctrl/Cmd-click, Shift-click for a range) to pick several stickers, then right-click one of them to add them all to a group at once. Groups also appear as icons at the bottom of the picker.
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
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libx11-dev libxrandr-dev libxi-dev libxkbcommon-x11-0 libgl1-mesa-dev libdbus-1-dev
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

If Picora closes unexpectedly, it writes the error to `crash.log` in the data folder. Open the folder from **Settings → Open data folder** and attach that file to your bug report.

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

The arca.live check uses your saved session. Windows viewport tests need an interactive desktop; the Wayland script requires headless KWin. For macOS browser clipboard checks, build `cargo build --locked --example clipboard_fixture` and follow the instructions in [smoke_macos_chrome.py](scripts/smoke_macos_chrome.py).

Packaging and installer scripts are used by CI. `test_update_manifest.py` validates release metadata; `tests/update_process.rs` runs updater replacement and recovery scenarios as part of `cargo test`. Platform smoke scripts cover desktop behavior that headless CI cannot exercise. See [release and recovery documentation](docs/UPDATES.md).

On Linux, desktop integration files are included in the archive. Recent Wayland portals require the desktop entry to identify Picora when registering global shortcuts. Put the binary on `PATH`, install the desktop entry to `~/.local/share/applications/`, and the icon to `~/.local/share/icons/hicolor/scalable/apps/`.

## Project

Picora uses Rust and egui. Repository adapters, SQLite storage, media caching, and native desktop integration are separate modules; website sign-in runs in a Wry helper process.

[Noto Sans KR](https://github.com/notofonts/noto-cjk/tree/main/Sans/SubsetOTF/KR) is bundled under the [SIL Open Font License](assets/OFL.txt). Sticker artwork is downloaded from its source repository at your request. Picora is not affiliated with the supported repositories.
