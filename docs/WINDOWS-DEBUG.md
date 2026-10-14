# Windows freeze: diagnosis and fix

## Reproduced on 2026-10-14

The original `eb95cdc` branch builds and passes its 16 non-network tests on
Windows, but the fresh library stops responding to both **Browse repositories**
and **Discover**. This also occurs in a native Rust 1.90 x64 MSVC debug build,
so it is not specific to the previously supplied GNU executable.

Input traces showed that App::update and the library viewport stopped running
after the root picker window was hidden. Leaving that window visible made the
same Browse action immediately load the live catalog. No repository request
or focus-command deadlock was involved.

## Root cause and patch

eframe 0.33.3 relies on native redraw events to update windows. Windows does not
deliver those events to a hidden window. Picora hides its root picker and
renders its library with `show_viewport_immediate`, which depends on root
updates. The library therefore stalls, activation events are not consumed,
and eframe spins its event loop.

Upstream fixed hidden/minimized-window handling in
[egui PR #7905](https://github.com/emilk/egui/pull/7905). Picora backports commit
`cd3c38cf2a0d65330a645faeadb6f666bc8df8f2` through `vendor/eframe` and a Cargo
patch override. The `new_events` path additionally establishes the active
event-loop context: opening a new immediate library viewport after background
startup otherwise panics. See `vendor/eframe/PICORA-PATCH.md` for provenance.

This keeps the 0.33 UI API and existing window behavior. Remove the override
when deliberately migrating to a newer eframe lifecycle API.

## Regression and verification

On an interactive Windows desktop:

```powershell
cargo test --locked
cargo test --locked --test windows_viewports -- --ignored --nocapture
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

The native regression runs each scenario in a subprocess with a 15-second
deadline. It verifies that the library can update after the root has been
hidden for two seconds and that the root actually becomes visible again.
It also covers a library first created after hidden/background startup.
The original dependency timed out. The upstream patch alone passed the first
scenario but failed the second; the context adjustment passes both.

Native app checks with an isolated data directory verified live DCInside
catalog and thumbnails, pack preview, a 17-sticker pack download, library
image copy, library close/reopen through `--library`, and Ctrl+Shift+Space
picker activation. Arca authentication and insertion into Discord/Telegram
remain separate acceptance work.

To isolate manual testing:

```powershell
$env:PICORA_DATA_DIR = "$PWD\test-data"
cargo run --locked -- --library
```

After closing the library, run `picora --library` or `picora --show` with the
same data directory to exercise activation while all windows are hidden.

## Arca session persistence

The old implementation serialized the entire cookie jar into one Credential
Manager password. Windows caps that blob at 2,560 bytes, and the keyring backend
encodes passwords as UTF-16. A synthetic 2,000-character value reproduces the
failure even when a small credential saves successfully. The UI previously
discarded this error and called the credential store unavailable.

Windows now uses current-user DPAPI encryption in `arca-session.dpapi` under
the Picora data directory. Only ciphertext is written, using atomic replacement.
Existing small Credential Manager sessions remain readable when no encrypted
file exists. Sign-out removes both stores. Other platforms retain keyring storage.

Tests cover a large cookie jar, fresh-process reload, replacement, failed-save
preservation, corruption rejection, and deletion. Run the original size-limit
reproduction explicitly; it creates and removes only a synthetic test credential:

```powershell
cargo test --locked --test windows_credentials -- --ignored
```

This verifies persistence, not an authenticated Arca catalog or download.
Reconnect once after upgrading from a memory-only session.
