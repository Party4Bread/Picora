# GitHub Self-Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Update Picora from GitHub Releases through verified application-file replacement on Windows, Linux and macOS, with rollback.

**Architecture:** An asynchronous release client feeds an egui update state machine. A separate invocation of a copied Rust executable owns transactional replacement after the desktop process exits, and confirms successful startup before discarding backups. Tag CI produces the manifest and platform archives consumed by the client.

**Tech Stack:** Existing Rust/Tokio/reqwest/serde/sha2/tempfile stack; add semver, zip and flate2/tar for version parsing and bounded archive extraction; native process handles on Windows; GitHub Actions and existing Python packaging.

**Spec:** ../specs/2026-10-14-self-updater-design.md

## Global Constraints

- Source: public `Party4Bread/Picora` GitHub Releases; stable `vMAJOR.MINOR.PATCH` only.
- Windows uses application-file replacement, not the installer. All platforms require an explicit **Update and restart** click.
- Initial targets: Windows x86_64, Linux x86_64, macOS aarch64. No architecture fallback.
- Preserve application data, credentials and unrelated installation files; never elevate automatically.
- Helper waits for the exact parent process; readiness acknowledgement precedes GUI shutdown. New app health acknowledgement has a 60-second deadline.
- Publish only after every required build/test succeeds. Do not create tags or publish a release during this implementation.
- Checksums trust repository release metadata; they are not publisher signatures.

## Review Focus

- Repeated clicks or stale background events must not start concurrent replacement (Task 5).
- A restarted machine or interrupted helper must recover an incomplete transaction (Tasks 3–4).
- Windows antivirus/file locks must lead to bounded retries and restoration rather than partial installation (Tasks 3–4).
- Installed Windows copies must retain uninstaller files; macOS archives must retain executable modes (Tasks 2–3).
- Old saved settings and absent GitHub releases must not break startup (Tasks 1 and 5).

## File map

Create `src/update/{mod,release,archive,transaction,helper}.rs`: shared types/state; HTTP/version selection; bounded extraction; journal/backup/recovery; process handoff respectively. Create `src/app/updates.rs` for update controls. Modify `src/{lib,main,model,services}.rs` and `src/app/{mod,events,library}.rs` only at integration points. Add `tests/update_{release,archive,transaction,helper}.rs`, fixture data under `tests/fixtures/update/`, and `examples/update_test_child.rs` for headless process tests. Extend packaging and add `scripts/update_manifest.py`, `scripts/test_update_manifest.py` and `.github/workflows/release.yml`. Document release/recovery operations in `docs/UPDATES.md`.

### Task 1: Release contract and asynchronous client

**Interfaces:** `Target::current() -> Option<Target>`; `select_update(current: &semver::Version, target: Target, release: Release, manifest: Manifest) -> anyhow::Result<Option<AvailableUpdate>>`; `ReleaseClient::check(&self, current: &semver::Version, target: Target) -> anyhow::Result<Option<AvailableUpdate>>`. Manifest schema 1 contains version and targets with archive name, size and sha256; AvailableUpdate retains verified metadata and release notes.

- [ ] Add failing fixture tests for 0.1.9→0.1.10, equal/older versions, drafts/prereleases, no releases, duplicate targets, mismatched tag/manifest, missing architecture, invalid digest, foreign-repository URL and malformed JSON.
- [ ] Run `cargo test --test update_release`; confirm the new behavior is absent.
- [ ] Implement schema validation and semantic comparison. Use production HTTPS-only URLs and validated GitHub asset redirects. Use a distinct injected transport for local mock tests; production must not accept arbitrary servers.
- [ ] Implement timeouts, rate-limit errors and bounded streaming: metadata 1 MiB, archive 256 MiB, 10-second connection timeout, 30-second metadata timeout, 5-minute archive timeout. Verify exact declared length and SHA-256 before extraction. Plain-text release notes are not executable markup.
- [ ] Test HTTP 404 as no release, 403/429 as retryable errors, interrupted/oversized responses and checksum failures against a local fixture server. Run `cargo test --test update_release` and commit this component.

### Task 2: Safe package staging

**Interfaces:** `stage_archive(archive: &Path, update: &AvailableUpdate, staging: &Path) -> anyhow::Result<StagedUpdate>`; StagedUpdate contains the target, version and validated relative application-file inventory. No installation mutation occurs here.

- [ ] Add failing ZIP/tar fixture tests for traversal, absolute/drive/UNC paths, duplicate names, case-insensitive Windows collisions, links, unexpected roots, missing executable and decompression limits. Assert rejected archives create nothing outside staging.
- [ ] Run `cargo test --test update_archive` and confirm failure before implementation.
- [ ] Implement extraction of only the specified Picora package layout, with a 1 GiB expanded-size and 10,000-entry ceiling. Preserve executable modes; explicitly validate the macOS bundle executable. Reject Windows reserved names and alternate data streams.
- [ ] Test correct Linux, Windows and macOS layouts, partial-download cleanup and executable modes. Run `cargo test --test update_archive`; commit staging implementation.

### Task 3: Journaled replacement and recovery

**Interfaces:** `InstallLayout::detect(executable: &Path) -> anyhow::Result<InstallLayout>`; `Transaction::prepare(layout: &InstallLayout, staged: &StagedUpdate) -> anyhow::Result<Transaction>`; `apply(&mut self)`, `rollback(&mut self)`, `commit(&mut self)` return `anyhow::Result<()>`; `recover(layout: &InstallLayout) -> anyhow::Result<()>`. A transaction records durable phases and per-file progress.

- [ ] Add failing filesystem tests for successful replacement, failure before/after each rename, repeated rollback, interrupted journals, read-only locations, retained uninstaller/unrelated files and untouched data directories.
- [ ] Run `cargo test --test update_transaction` to confirm failure.
- [ ] Implement private same-filesystem staging and backups, exclusive update lock, canonical layout checks, symlink rejection and flushed journal transitions. Replace the complete macOS bundle; update only inventoried Windows/Linux app files. Reserve journal paths outside the replaced bundle.
- [ ] Retain an installed-file inventory so files removed by a future version can be removed transactionally without deleting unrelated files. Validate journal paths on every recovery; never trust arbitrary serialized absolute paths.
- [ ] Inject filesystem faults into tests, including bounded retry exhaustion for locked files, disk-full simulation, and crash points between rename and journal update. Run all transaction tests and commit.

### Task 4: Helper handoff and health acknowledgement

**Interfaces:** `launch_helper(job: UpdateJob) -> anyhow::Result<ReadyHelper>`; `run_helper(job_path: &Path) -> anyhow::Result<()>`; `acknowledge_health(token: &str) -> anyhow::Result<()>`. UpdateJob includes validated staging/install identities, parent identity and nonce; ReadyHelper is returned only after authenticated readiness.

- [ ] Add failing integration tests with `examples/update_test_child.rs`: parent still alive, readiness failure, timeout, concurrent helper, healthy child, child crash, child hang and interrupted recovery.
- [ ] Run `cargo test --test update_helper` to confirm failure.
- [ ] Dispatch internal helper/recovery modes before GUI and single-instance acquisition in `src/main.rs`. Copy the helper outside the install root; use private job permissions and nonce handshake. Wait on an exact Windows process handle; use a parent-lifetime pipe on Unix to avoid PID-reuse polling.
- [ ] Wait at most 30 seconds for parent exit, then perform Task 3 transaction. Retry transient replacement locks for at most 10 seconds. Spawn the installed executable with a one-time health token; acknowledge only after storage plus GUI initialization. On failure within 60 seconds terminate only the spawned child, rollback and restart the old version without a health token. Remove helper files on a later successful launch where self-deletion is unavailable.
- [ ] Integrate startup journal recovery before normal app initialization; acquire the same update lock so a normal launch cannot race a replacement. Persist a failure report for display in Settings.
- [ ] Run native headless protocol tests on each CI OS, including a full test-child upgrade/rollback. Commit helper and startup integration.

### Task 5: Settings and async update lifecycle

**Interfaces:** `UpdateState` models Idle, Checking, Current, Available, Downloading, Staged, HandingOff and Error. `UpdateEvent` carries a monotonically increasing operation id. `Services::check_update(id)` and `Services::prepare_update(id, update)` deliver events through the existing Bus; an explicit handoff action invokes Task 4.

- [ ] Add failing tests for defaults when deserializing old Settings, disabled startup checks, double clicks, stale events, failed helper readiness, cancelled downloads and state after errors.
- [ ] Run the relevant tests before implementation.
- [ ] Add `check_updates_automatically` with serde default true. Implement a single asynchronous startup check and Settings controls for version, preference, check, release notes, progress and **Update and restart**. Keep update status distinct from sticker operation status.
- [ ] Use operation ids and cancellation to ignore stale completion events. Once handoff starts, cancel pending automatic paste; quit only after ReadyHelper. Keep GUI active on every earlier failure. Disable overlapping update actions and display recovery reports. Unsupported layouts/targets get a release-page link and no replacement action.
- [ ] Run UI/state regressions and existing tests. Smoke-test Settings responsiveness with a slow/failing mock transport; commit UI integration.

### Task 6: Release automation and end-to-end verification

**Files:** `scripts/package.py`, `scripts/update_manifest.py`, `scripts/test_update_manifest.py`, `.github/workflows/release.yml`, existing CI, `docs/UPDATES.md`, README and compatibility documentation.

- [ ] Add failing Python tests for normalized target names, tag/version mismatch, missing/duplicate required assets, deterministic SHA-256/size generation and manifest compatibility with Rust fixtures. Run `python -m unittest discover -s scripts -p 'test_update_manifest.py'`.
- [ ] Clear package staging before each build; create normalized archives for explicit x86_64 Windows/Linux and aarch64 macOS runner targets. Preserve macOS ZIP modes and bundled license files. Keep the Windows installer as a separate first-install asset.
- [ ] Add tag-triggered release CI: validate stable tag/Cargo version; run fmt/test/Clippy, release build and packaging per target; collect only current-run artifacts; validate the complete manifest; create a draft release and upload every asset before publishing. Use contents:write only in the publish job. Release tags must point to reviewed workflow code on the repository default branch; fail clearly otherwise. No tag is created during implementation.
- [ ] Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, Python packaging tests and release builds. Verify Linux native helper replacement/rollback locally, then Windows/macOS native CI tests. Inspect all job results after pushing implementation commits; fix reported failures.
- [ ] Document first manual upgrade, release operation, direct replacement, writable-install requirement, Windows uninstall display-version limitation, recovery, checksum trust and unsupported architectures. Record real desktop smoke separately from headless CI evidence.
- [ ] Perform whole-branch review focused on process synchronization, interrupted recovery, extraction boundaries and installer preservation. Fix findings, rerun affected checks and commit final changes. Report remaining native desktop verification limits accurately.

## Self-review

All spec sections map to tasks: release/verification (1, 2, 6), UI/settings (5), process lifecycle (4), replacement/recovery (3, 4), automated/native acceptance (each task, then 6). Dependencies are sequential: 1 → 2 → 3 → 4 → 5 → 6. No actual release publication is needed to test the updater. Execution should retain one owner for transaction/helper interfaces and use an independent final review.
