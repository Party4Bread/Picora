# Picora — Desktop Sticker Picker for Windows, macOS & Linux

**Your favorite reactions, one shortcut away.** Picora is a Rust desktop sticker picker inspired by the Windows emoji panel. Browse **DCInside stickers (디시콘)** and **Arca stickers (아카콘)**, organize your favorites, and copy or paste image reactions into apps that accept clipboard images or file attachments.

Press **Ctrl+Shift+Space**, find a sticker, and choose it. Picora copies it, hides the picker, and attempts to paste into your previous app. It never presses Enter to send your message.

**[Download Picora](https://github.com/Party4Bread/Picora/releases/latest)** · **[Report a bug](https://github.com/Party4Bread/Picora/issues)** · **[Compatibility](docs/COMPATIBILITY.md)**

## Why Picora?

- **Bring your sticker collections to the desktop.** Browse DCInside and Arca packs from one library instead of hunting for individual image files.
- **Find the right reaction quickly.** Search sticker and pack titles, switch collections from thumbnail shortcuts, and use keyboard navigation in a compact picker.
- **Make the library yours.** Favorite individual stickers, create custom groups, and manage stickers with a right-click. Recent shows recently used stickers first, followed by newly added ones.
- **Keep still and animated reactions.** Copy a PNG frame or paste an animated sticker as a file. Original-file mode preserves the source bytes; playback and attachment behavior depend on the receiving app.
- **Keep downloaded stickers close.** Cached media and library metadata stay on your computer, so downloaded stickers remain available offline.
- **Stay up to date.** Install GitHub releases through **Settings → Updates → Update and restart**, preserving your library and settings.

Picora is written in Rust with a native egui interface, light and dark themes, and bundled Korean typography.

## Download and install

Get the latest build from **[GitHub Releases](https://github.com/Party4Bread/Picora/releases/latest)**. Choose the asset for your platform:

| Platform | Download | Installation |
| --- | --- | --- |
| Windows x64 | `picora-<version>-windows-x64-setup.exe` | Run the installer. Installs for your account without administrator rights. A portable ZIP is also available. |
| macOS, Apple Silicon | `picora-<version>-macos-aarch64.dmg` | Open the DMG and drag Picora to Applications. See the signing limitation below. |
| Linux x64 | `picora-<version>-linux-x86_64.tar.gz` | Extract and run `Picora/picora --library`. Requires the system libraries listed under building from source. |

**macOS:** builds are ad-hoc signed, but not Developer ID signed or Apple-notarized. Gatekeeper may block downloaded copies. Intel Macs are not covered by the current release artifact. Read [macOS installation and signing limitations](docs/MACOS-INSTALL.md) before installing.

**Windows:** the installer is unsigned. Arca sign-in requires the WebView2 runtime. Quit Picora from its tray menu before manual upgrades or uninstalling. Uninstalling preserves downloaded packs, settings, and encrypted sessions.

**Linux:** the picker requires X11 or XWayland. Native Wayland clipboard copying is supported through `wl-clipboard-rs` on compatible compositors and tested with KDE KWin; the windows still use XWayland.

Looking for an unreleased build? Successful [Rust desktop workflow runs](https://github.com/Party4Bread/Picora/actions/workflows/ci.yml) provide platform artifacts for testing.

## Start collecting in three steps

1. **Add a pack.** Open the library from Picora's tray/menu-bar icon, then **Discover**. Search DCInside or paste a pack URL, such as `https://dccon.dcinside.com/#41982` or `https://arca.live/e/54950`. Select the pack and choose **Add pack to favorites**.
2. **Organize your reactions.** Create collections in **Groups**. Right-click any sticker to favorite/unfavorite it, add it to a group, or remove it from the library.
3. **Use a sticker.** Focus your chat or editor, press **Ctrl+Shift+Space**, and click a sticker or select it with arrows and Enter. If automatic paste is unavailable, paste manually with **Ctrl+V** or **Cmd+V**.

Picora starts in the system tray/menu bar. Closing a window keeps it running; use **Quit** in the tray menu or Settings to exit. On Windows, check the taskbar's hidden-icons arrow. The shortcut is configurable, and launch at login is opt-in. The library window copies stickers without automatic paste.

### DCInside and Arca accounts

DCInside packs come from [DCInside's sticker repository](https://dccon.dcinside.com/). For [Arca sticker packs](https://arca.live/e/), open **Accounts → Sign in / reconnect**, sign in on the actual website, and close that window to return to Picora. Arca may require a browser challenge or renewed sign-in.

Windows stores Arca cookies encrypted for your Windows account in `arca-session.dpapi`; other platforms use the OS credential store. If saving fails, the session stays in memory and Picora shows an error. [Compatibility notes](docs/COMPATIBILITY.md) distinguish verified Arca flows from remaining desktop-specific checks.

### Animated stickers and library behavior

Hover to preview animation. Normal selection pastes animated GIF/WebP as files; APNG and supported MP4 stickers can be converted to animated GIF for attachment compatibility. **Paste original file** preserves the original bytes. **Copy still image** copies a PNG frame. GIF conversion can reduce colors and partial transparency; use the original when the receiving app supports it.

Favorites save before downloads finish. If a download is interrupted, the pack remains in your library; uncached stickers download when used, or you can retry the pack download. Removing a sticker also removes its group membership and history; reimport its source pack to restore it. Cached files are retained so existing clipboard file references remain usable.

Reimporting a pack does not make its existing stickers appear newly added. Older libraries without import timestamps initially use their stored insertion order.

## Desktop integration

| Desktop | What to expect |
| --- | --- |
| Windows | Global shortcut and automatic paste. Elevated target apps may reject paste injection. |
| macOS | Grant Accessibility permission to Picora for automatic paste, or to the terminal for an unpackaged source build. Downloaded-app signing limitations still apply. |
| Linux X11 | Global shortcut, focus restoration, and X11 clipboard ownership. |
| KDE Plasma / Wayland | Native image/file clipboard access on data-control-capable compositors. Approve the desktop shortcut dialog and enable **Settings → Wayland keyboard access** for automatic paste. XWayland is required for picker windows. |
| Other Wayland desktops | Clipboard and portal support vary. Picora retains portal/XWayland clipboard fallbacks. If the shortcut portal is unavailable, bind `picora --show` in desktop settings. |

Copying through Wayland data-control does not need Remote Desktop permission. Automatic paste does: the portal requests keyboard access only. Picora tries an already connected clipboard portal first, then native data-control, then the XWayland clipboard bridge. Declined input permission leaves manual paste available where clipboard access works.

No single clipboard format works in every chat app. See [tested behavior and manual acceptance checks](docs/COMPATIBILITY.md) for the current verification record.

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
# Optional: -Compiler 'C:\path\to\ISCC.exe'
```

Platform smoke tests run explicitly:

```sh
# KDE clipboard test in a private headless KWin session:
python scripts/smoke_wayland_clipboard.py
# Windows interactive picker/library regression:
cargo test --locked --test windows_viewports -- --ignored --nocapture
# Live DCInside access:
cargo test --locked --test live live_dcinside_search_pack_and_image -- --ignored
```

CI builds and tests Windows, macOS, and Linux once per pull request or main-branch push. Superseded CI runs are cancelled; successful main builds seed dependency caches for PRs and releases. CI artifacts are retained for seven days. Version tags run the matrix through the release workflow only, which publishes packages and an updater manifest after all platform checks pass. Published release assets are not subject to CI artifact retention. See [release, update, and recovery details](docs/UPDATES.md).

On Linux, optional desktop integration files are included in the archive. Put the binary on `PATH`, install the desktop entry to `~/.local/share/applications/`, and the icon to `~/.local/share/icons/hicolor/scalable/apps/`.

## Project notes

Picora separates repository adapters, SQLite storage, media caching, background services, native platform integration, and the egui UI. Arca sign-in runs in a separate Wry helper process.

Noto Sans KR is bundled under the [SIL Open Font License](assets/OFL.txt), sourced from [Noto CJK](https://github.com/notofonts/noto-cjk/tree/main/Sans/SubsetOTF/KR). Sticker artwork is downloaded from its original repositories at your request; no sticker artwork is bundled. Picora is an independent project, not an official DCInside or Arca client.

**한국어:** Picora는 Windows·macOS·Linux용 데스크톱 스티커 선택기입니다. 디시콘과 아카콘을 즐겨찾기와 사용자 그룹으로 정리하고, 단축키로 열어 이미지나 파일을 클립보드에 복사할 수 있습니다. Arca 로그인, 운영체제 권한, 대상 앱의 붙여넣기 지원 여부에 따라 일부 기능이 제한될 수 있습니다.
