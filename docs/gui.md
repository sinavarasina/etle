# GUI

`etle-gui` is a GTK4/Relm4 desktop interface for ETLE.

## Build

```bash
cargo build --locked --release --no-default-features --features gui-relm4 --bin etle-gui
```

## Run

```bash
./target/release/etle-gui
```

## Version

```bash
./target/release/etle-gui --version
```

## Runtime Requirements

Linux/macOS:

- GTK4 must be installed by the system package manager.

Windows release package:

- GTK4 runtime DLLs and data directories are bundled in the portable zip.

## Relationship to Daemon

The GUI talks to `etled` through IPC. It does not replace the daemon.

Typical flow:

```text
start/open GUI
connect/sync with daemon IPC
list shares
seed file
download share
watch logs/progress
```

## PSK Notes

The GUI download PSK field is for client/download commands. To make a seeder require PSK, start the daemon itself with:

```bash
etled -v serve --auth-psk "same-password"
```

Then use the same PSK in the GUI download form.

## Current UX Limitations

- Some settings apply to future commands, not already-running daemon state.
- Long-running operation cancellation is still being improved.
- Error messages should be expanded over time.
- Cross-platform styling may differ because GTK runtime/theme differs.
