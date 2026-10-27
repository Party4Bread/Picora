# Installing Picora on macOS

The macOS download is for Apple Silicon (M-series) Macs. Intel Macs are not supported by this release artifact.

Open the `.dmg` and drag **Picora.app** to **Applications**, then eject the disk image and open Picora from Applications. The ZIP is also available: extract it and move `Picora/Picora.app` to Applications. Picora runs as a menu-bar utility; it does not keep a Dock icon. Use its menu-bar menu to open the library.

## Automatic paste

In Picora, open **Settings → Automatic paste on macOS → Allow automatic paste…**. Enable **Picora** in **System Settings → Privacy & Security → Accessibility**, return to Picora and choose **Check permission again**. Select the sticker again. If access is still unavailable, restart Picora.

Permission carries over to later updates. If Picora is already turned on in the list but automatic paste still does not work, macOS is allowing an older build: remove Picora with **−**, add the current **Picora.app** with **+**, turn it on, and reopen Picora. This is needed once when updating from a version signed before this change (0.1.9 and earlier).

If no image appears, focus the editor and press **Cmd+V**, or try **Copy still image**.

## Current signing limitation

Release builds are signed with Picora's own self-signed certificate. It seals the bundle and gives every release the same signing identity, which lets macOS keep Accessibility permission across updates. It is **not a Developer ID signature, and builds are not notarized by Apple**. A DMG is an installation convenience; it does not bypass Gatekeeper. macOS may block a downloaded build. Do not disable Gatekeeper globally. See Apple's guidance for opening apps from unidentified developers: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac

If macOS says the app is damaged, stop and report the exact message, macOS version and downloaded filename. This message alone does not establish whether a download is corrupt or macOS rejected its signature. Version 0.1.2 did not seal the app bundle. The packaging fix adds a seal and native package validation, but does not establish Developer ID trust.

The release maintainer needs an Apple Developer membership, a Developer ID Application certificate, and notarization credentials to provide the standard trusted download-and-open experience. These credentials are not currently configured. Do not send private keys or passwords in an issue or chat.

## Verification boundaries

CI checks ZIP integrity, executable permissions, bundle metadata and signature verification, then runs the extracted executable with isolated data and checks it survives startup. It also verifies the DMG. This is not a quarantined Finder/Gatekeeper launch test, and does not certify macOS permission prompts, hotkeys, paste, or updates on a user's Mac.
