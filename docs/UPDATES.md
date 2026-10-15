# Updating Picora

Settings → Updates checks stable GitHub Releases from Party4Bread/Picora. Automatic startup checks are enabled by default and can be disabled. Checks run in the background. Nothing is installed until **Update and restart** is clicked. Cancelling the download leaves the installed app unchanged.

Updates replace application files on Windows, Linux and macOS. The Windows installer is only for first installation; the updater does not invoke it. Both installed and portable Windows copies keep their existing location. The uninstaller remains intact, and the helper holds the installer mutex while replacing files; Windows Installed Apps may show the original installer version until an installer upgrade. Picora itself shows its current version.

The running installation must be writable. No administrator elevation is requested. On macOS, run the extracted .app from a writable directory, not inside a read-only disk image. Supported release targets are Windows x86_64, Linux x86_64 and macOS Apple Silicon (aarch64). Other targets can still build from source but receive no automatic replacement.

A separate helper waits for Picora to exit, keeps backups, replaces files, and starts the new version. The new app acknowledges successful storage and desktop initialization. A failed start or acknowledgement timeout (60 seconds) restores the previous files. Application data, sticker collections and credentials are outside this transaction and are preserved. Future database migrations must retain compatibility with the preceding app version for rollback to remain useful.

## Recovery

The installation's parent directory contains `.<installation-name>.picora-update`, holding the transaction lock, journal and backups. An ordinary launch recovers an interrupted transaction before opening storage. Do not delete that directory while an update or recovery is in progress. File-lock failures retry for up to 10 seconds. A still-running helper owns the transaction lock; wait for it to finish before launching another copy.

If recovery cannot finish, retain the directory and resolve permissions/file locks before trying again. `last-error.txt` records updater failures and is shown in Settings. Helper temporary directories are removed on Unix after completion; Windows removes abandoned helper directories on a later ordinary launch. A killed updater before file replacement leaves the installed version unchanged.

## Publishing a release

1. Merge reviewed updater/workflow changes into the repository default branch. Bump Cargo.toml and Cargo.lock to a stable version, and commit it.
2. Push the corresponding `vMAJOR.MINOR.PATCH` tag. The tag must match Cargo.toml and point to a commit reachable from the default branch.
3. The Publish release workflow runs the Linux, Windows and macOS build/test matrix. It collects archives and the first-install Windows installer, then generates `update-manifest.json` (schema 1) with exact target, filename, length and SHA-256.
4. Only after successful builds does it create a draft, upload all assets, and publish the release. If upload/publication fails, inspect the draft before retrying; do not publish an incomplete draft.

Archives use names `picora-VERSION-windows-x86_64.zip`, `picora-VERSION-linux-x86_64.tar.gz`, and `picora-VERSION-macos-aarch64.zip`. Preserve those names and the manifest. The app rejects mismatched versions, unsupported targets, malformed manifests and checksum failures.

Downloads use HTTPS and trust this GitHub repository's release metadata. Checksums detect corrupt or mismatched downloads; they are not independent code signatures. Signing/notarization and delta updates are not included. Installations predating the updater require one manual upgrade to gain this feature.

## Verification

`cargo test --locked` includes native headless helper tests through `examples/update_test_child.rs`: real parent exit, replacement, acknowledgement, rollback on crashed or hung startup, and external recovery after an interrupted transaction. Filesystem and archive tests cover preservation, recovery and unsafe paths. `python -m unittest discover -s scripts -p test_update_manifest.py` checks the release contract. These tests do not substitute for a full native desktop update smoke test on each OS; consult the implementation report for which runs were actually observed.
