# Compatibility

Picora supports Windows, macOS, and Linux. Clipboard formats, automatic paste, and global shortcuts depend on the desktop and receiving app.

## Desktop limitations

- **Windows:** elevated apps may reject automatic paste. Use manual paste when needed.
- **macOS:** automatic paste requires Accessibility permission. Downloads are not Developer ID signed or notarized; see [installation instructions](MACOS-INSTALL.md).
- **Linux X11:** keep Picora running in the tray to retain clipboard ownership.
- **Wayland:** picker windows require XWayland. Shortcut and keyboard access depend on desktop portals; Picora attempts to restore saved keyboard permission after restart when the desktop and OS credential store support it. If restoration fails, reconnect in Settings. If the shortcut portal is unavailable, bind `picora --show` in desktop settings. Automatic paste also depends on the compositor restoring focus to the target app.
- **Wayland clipboard:** native image/file copying requires compositor data-control support. Portal and XWayland fallbacks are available. Copying does not itself require keyboard permission.
- **KDE Plasma + fcitx5:** repeated Korean syllables have been regression-tested under X11, but confirmation on a real Plasma desktop remains outstanding.

## Stickers and accounts

- The receiving app decides whether a paste becomes an image, animation, or file. If animation fails, try **Paste original file** or **Copy still image**. Discord and Telegram behavior still needs verification on each desktop.
- arca.live requires sign-in and may request renewed browser verification. Embedded login and session persistence have not been verified on every OS.
- Animation previews stop at 120 frames or 64 MiB of decoded data; original files remain intact. MP4 conversion supports non-fragmented H.264, up to 4096×4096, two minutes, 2,000 frames, and 64 MiB decoded data. Unsupported videos return an error.

See [desktop setup](../README.md#desktop-integration) and [self-updates](UPDATES.md) for configuration details. Report issues with your Picora version, OS/desktop, receiving app, and steps to reproduce.
