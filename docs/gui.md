# GUI

`etle-gui` is a GTK4/Relm4 desktop interface for ETLE.

## Build

```bash
cargo build --locked --release --no-default-features --features gui-relm4 --bin etle-gui
```

For development checks:

```bash
cargo check --locked --no-default-features --features gui-relm4 --bin etle-gui
cargo clippy --locked --no-default-features --features gui-relm4 --bin etle-gui -- -D warnings
```

## Run

```bash
./target/release/etle-gui
```

## Version

```bash
./target/release/etle-gui --version
```

## Architecture

`etle-gui` is structured with declarative Relm4 (`relm4::Component` / `SimpleComponent` / `FactoryComponent` with `view!`, `#[watch]`, `#[block_signal]`, and `FactoryVecDeque`):

- Root shell (`src/gui/app.rs`) orchestrates header/status controls, banner/transfer strip, `gtk::Stack` navigation, and Relm4 async Tokio commands (`oneshot_command` / `command`).
- Modular pages (`src/gui/pages/{library,seed,download,activity,settings}.rs`) own their GTK buffers, adjustments, and `FactoryVecDeque` lists.
- Pure Rust domain/sync state (`src/gui/state.rs`, `src/gui/model.rs`) is isolated from GTK widgets so state/daemon reconciliation tests run headless.

## Runtime Requirements

Linux/macOS:

- GTK4 must be installed by the system package manager.

Windows release package:

- GTK4 runtime DLLs and data directories are bundled in the portable zip.
- The app installs shared theme-adaptive CSS (`src/gui/style/common.css`) on all platforms and Windows-only Fluent-like GTK CSS (`src/gui/style/windows.css`) on Windows.
- The app sets the Windows GTK decoration environment before GTK/Relm4 initialization when possible.

## Relationship to Daemon

The GUI talks to `etled` through IPC (`GetStatus`, `ListShares`, `SubscribeEvents`, and command RPCs). It does not replace the daemon.

Typical flow:

```text
start/open GUI
sync live daemon status + share list (and subscribe to IPC event stream)
list shares and inspect structured metadata / resume incomplete downloads
seed single or queued files with chunk presets
download share (with collapsible Advanced & Discovery options)
delete local library share (blocked while an active download uses that share)
watch active transfers (Seed, Download, Upload) and logs
inspect live daemon runtime settings / stop daemon from Settings
```

The Library page uses daemon IPC for share deletion and shows an inline confirmation panel before sending a destructive request. Selection persists by `ShareId` across refreshes.

## PSK Notes

The Settings PSK field stores the default GUI download PSK. The Download page's optional PSK override applies only to that download request without overwriting Settings. To make a seeder require PSK, start the daemon itself with:

```bash
etled -v serve --auth-psk "same-password"
```

Then use the same PSK in Settings or the GUI download form.

## Current UX Limitations

- Daemon listen address, discovery config, and daemon PSK are set when starting `etled`; the Settings page shows the live runtime values reported by `etled`.
- Long-running operation cancellation is still being improved.
- Cross-platform styling may differ because GTK runtime/theme differs.
- Windows styling is app-scoped and Fluent-like, not a native WinUI rewrite.
