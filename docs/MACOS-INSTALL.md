# Installing Picora on macOS

The macOS download is for Apple Silicon (M-series) Macs. Intel Macs are not supported by this release artifact.

Open the `.dmg` and drag **Picora.app** to **Applications**, then eject the disk image and open Picora from Applications. The ZIP is also available: extract it and move `Picora/Picora.app` to Applications. Picora runs as a menu-bar utility; it does not keep a Dock icon. Use its menu-bar menu to open the library.

## Current signing limitation

Current builds are ad-hoc signed for bundle integrity, but are **not Developer ID signed or notarized by Apple**. A DMG is an installation convenience; it does not bypass Gatekeeper. macOS may block a downloaded build. Do not disable Gatekeeper globally. See Apple's guidance for opening apps from unidentified developers: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac

If macOS says the app is damaged, stop and report the exact message, macOS version and downloaded filename. This message alone does not establish whether a download is corrupt or macOS rejected its signature. Version 0.1.2 did not seal the app bundle. The packaging fix adds a seal and native package validation, but does not establish Developer ID trust.

The release maintainer needs an Apple Developer membership, a Developer ID Application certificate, and notarization credentials to provide the standard trusted download-and-open experience. These credentials are not currently configured. Do not send private keys or passwords in an issue or chat.

## Verification boundaries

CI checks ZIP integrity, executable permissions, bundle metadata and signature verification, then runs the extracted executable with isolated data and checks it survives startup. It also verifies the DMG. This is not a quarantined Finder/Gatekeeper launch test, and does not certify macOS permission prompts, hotkeys, paste, or updates on a user's Mac.
