# Build and Release

ETLE releases are produced by GitHub Actions from semantic version tags.

## Tag Format

Use:

```text
vMAJOR.MINOR.PATCH
```

Example:

```bash
git tag -a v1.0.0 -m "ETLE v1.0.0"
git push origin v1.0.0
```

Do not use `v1.0` because the workflow expects `v*.*.*`.

## Local Pre-release Checks

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Build release binaries:

```bash
cargo build --locked --release --bin etle-cli --bin etled
cargo build --locked --release --no-default-features --features gui-relm4 --bin etle-gui
```

Check build info:

```bash
./target/release/etle-cli --version
./target/release/etled --version
./target/release/etle-gui --version
```

## Release Workflow

Expected outputs:

```text
etle-v1.0.0-x86_64-unknown-linux-gnu-portable.tar.gz
etle-v1.0.0-aarch64-apple-darwin-portable.tar.gz
etle-v1.0.0-x86_64-pc-windows-gnu-portable.zip
CHECKSUMS.txt
```

Linux/macOS packages are `.tar.gz`.

Windows package is `.zip` and includes:

- ETLE binaries
- MSYS2 UCRT64 GTK4 DLLs
- common GTK runtime data directories
- fontconfig/gdk-pixbuf/GTK data when available

## Package Contents

Each package should contain:

```text
etle-cli / etle-cli.exe
etled / etled.exe
etle-gui / etle-gui.exe
PACKAGE-NOTES.md
BUILD-INFO.txt
SHA256SUMS.txt
README.md
LICENSE*
```

## Checksums

Two checksum layers are used:

1. `SHA256SUMS.txt` inside each package for binaries.
2. Release-level `CHECKSUMS.txt` for archives.

A binary cannot reliably contain its own final checksum because embedding the checksum would change the binary bytes.

## Manual Workflow Dispatch

Use GitHub UI:

```text
Actions → Release → Run workflow
```

Inputs:

```text
ref: main or tag
publish: true/false
release_tag: v1.0.0
draft: true
prerelease: false
```

For dry-run package testing, use:

```text
publish: false
```

## Release Verification Checklist

After workflow finishes:

- [ ] Download Linux archive.
- [ ] Download macOS archive if needed.
- [ ] Download Windows zip.
- [ ] Verify `CHECKSUMS.txt`.
- [ ] Extract each package.
- [ ] Run `etle-cli --version`.
- [ ] Run `etled --version`.
- [ ] Run `etle-gui --version`.
- [ ] Start daemon.
- [ ] Seed/list/download locally.
- [ ] Test discovery on LAN.
- [ ] Launch GUI.
