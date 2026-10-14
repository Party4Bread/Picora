# GitHub self-updater design

## Agreed intent

Picora checks GitHub Releases for updates, shows the available version and release notes, and updates only when the user selects **Update and restart**. All supported platforms use application-file replacement, including Windows (explicit user preference). Preserve the sticker library, groups, credentials and settings. The installer remains a first-install distribution option, not the update mechanism.

## Release contract

Use the public Party4Bread/Picora repository and stable semantic version tags `vMAJOR.MINOR.PATCH`. Ignore drafts, prereleases, equal versions and downgrades. Query the GitHub Releases API without credentials; show an informative state for no published release, rate limiting, offline failures or unavailable target assets.

Tag CI validates that the tag matches Cargo.toml, builds/tests every supported target, packages archives, generates a versioned update manifest containing each target's exact archive filename, size and SHA-256, and publishes only after all required jobs succeed. Normalize architecture identifiers rather than using platform.machine() in update selection. Initial release targets are Windows x86_64, Linux x86_64 and macOS aarch64, matching explicit CI runners. Unsupported architectures show a release-page link instead of accepting a different binary.

The manifest and archive are fetched via HTTPS from this repository's release assets. Bound metadata/download/extracted sizes, validate redirects to GitHub asset delivery hosts, reject mismatched hashes and duplicate target entries. Checksums establish integrity against the trusted release metadata, not independent publisher signing. Code signing and notarization remain separate work.

Reference: https://docs.github.com/en/rest/releases/releases

## In-app flow

Settings shows the running version, automatic-check preference (default enabled), **Check for updates**, last result, release notes, and **Update and restart** when a newer matching release exists. Check asynchronously once after startup; never block rendering or show a modal on startup network failure. Keep manual checks available when automatic checks are disabled. Prevent overlapping checks/downloads/installations. Show download progress and errors with retry. Cancellation before handoff removes staging files and leaves the installed app untouched.

State transitions: idle → checking → current / available / error; available → downloading → verifying → staged → handoff. A download error returns to a retryable state. Quit only after the helper acknowledges readiness; a failed helper launch leaves the current app running. Suppress automatic sticker paste while update shutdown is pending.

## Replacement and recovery

Implement the updater/helper in Rust. The running app stages the verified archive and extracts only expected application files into a private directory. Reject absolute/traversal paths, symlinks, hard links, special files and duplicate archive paths. Do not extract over the installed app. Preserve Unix executable permissions and the macOS .app layout.

Copy the current executable to a private temporary helper location and start it in an internal update mode before normal GUI/single-instance initialization. Pass a validated update job through a private file or inherited pipe, with a per-job nonce for the readiness handshake. The helper waits for the exact parent process to terminate with a bounded timeout, then takes the application update lock. Windows must wait using a process handle, not a guessed delay. Recheck install paths and reject symlink redirection before replacement.

Resolve Windows/Linux installation roots from the executable and macOS roots from the enclosing Picora.app. Stage replacement files on the destination filesystem for atomic rename. A writable installation is required; do not elevate automatically. Keep Windows uninstaller files and installation registration intact, and replace only application-owned files from the validated package manifest. Portable Windows ZIP installs use the same operation. Never modify the separate application-data directory.

Maintain a durable transaction journal and backup of replaced files. Restore on replacement failure or an interrupted transaction. Restart the new app with a one-time update health token; it acknowledges only after storage and GUI initialization succeed. If it exits or fails to acknowledge within 60 seconds, terminate only that replacement child, restore the previous files and restart the previous version. Preserve failure details for the next launch. Keep the backup until acknowledgement; clean it and temporary files afterward. Recovery must be idempotent across helper interruption and machine restart.

macOS replaces the complete bundle at its original location, preserving the archive's bundle contents. Read-only disk images, unwritable installations and unsupported layouts produce an actionable error and release link without changing files. Updating an installer-managed Windows copy preserves its install location; its registered uninstall display version may remain at the originally installed version until a subsequent installer run. The app version remains authoritative.

## Tests and acceptance

- Fixture tests: semantic versions, missing releases/assets, malformed manifests, wrong platform, unsafe URLs, download bounds, hash mismatch and archive traversal/link entries.
- Filesystem tests: successful replacement, failure at each rename step, retained unrelated/uninstaller files, permission failures, interrupted-journal recovery and unchanged app data.
- Helper integration tests: waits for parent exit, readiness failure, lock contention, successful restart acknowledgement, failed health check and rollback. Use purpose-built test children so CI does not depend on an interactive desktop for protocol tests.
- UI regression tests: asynchronous update state handling, repeated clicks and errors. Check formatting, Clippy and tests on Linux, Windows and macOS.
- Native release smoke: real old-to-new application replacement and startup on each platform. Record separately from headless helper tests; never claim native behavior based only on cross-compilation.

## Scope and execution boundary

Implement the app, helper, tests and release workflow after spec/plan approval. Do not create a release tag or publish an actual release as part of implementation. Existing installations without updater code require one manual upgrade to receive it. No installer invocation, silent installation, delta updates, custom update server or GitHub login is required.
