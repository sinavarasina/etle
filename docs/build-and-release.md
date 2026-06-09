# Build and Release

ETLE releases are produced by GitHub Actions from semantic version tags. Release binaries are built with `--locked`, so `Cargo.toml` and `Cargo.lock` must be in sync before tagging.

## Tag Format

Use:

```text
vMAJOR.MINOR.PATCH
```

Example:

```bash
git tag -a v1.0.1 -m "ETLE v1.0.1"
git push origin v1.0.1
```

Do not use `v1.0` because the workflow expects `v*.*.*`.

## Local Pre-release Checks

Run formatting, checks, tests, and clippy with the same locked dependency mode used by CI:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Check the GUI-only feature set:

```bash
cargo check --locked --no-default-features --features gui-relm4 --bin etle-gui
cargo clippy --locked --no-default-features --features gui-relm4 --bin etle-gui -- -D warnings
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

Optional benchmark compile check:

```bash
cargo bench --no-run
```

If Cargo reports that it cannot update `Cargo.lock` because `--locked` was passed, regenerate the lock file locally and commit it:

```bash
cargo generate-lockfile
git add Cargo.toml Cargo.lock
```

## Release Workflow

Expected outputs for a `v1.0.1` release:

```text
etle-v1.0.1-x86_64-unknown-linux-gnu-portable.tar.gz
etle-v1.0.1-aarch64-apple-darwin-portable.tar.gz
etle-v1.0.1-x86_64-pc-windows-gnu-portable.zip
CHECKSUMS.txt
```

Linux/macOS packages are `.tar.gz`.

Windows package is `.zip` and includes:

- ETLE binaries
- MSYS2 UCRT64 GTK4 DLLs
- common GTK runtime data directories
- fontconfig/gdk-pixbuf/GTK data when available

The package build injects release metadata through environment variables such as `ETLE_RELEASE_TAG` and `ETLE_BUILD_DATE`. Runtime `--version` output should show the tag, commit, target, build profile, and CI metadata when available.

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
release_tag: v1.0.1
draft: true
prerelease: false
```

For dry-run package testing, use:

```text
publish: false
```

For a real release, create or select the tag, set `publish: true`, and ensure `release_tag` matches the tag.

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
- [ ] Start daemon with `etled -v serve`.
- [ ] Seed a small file.
- [ ] List shares.
- [ ] Delete a test share.
- [ ] Download locally using discovery.
- [ ] Download locally using explicit `--peer`.
- [ ] Launch GUI and verify Library/Seed/Download/Settings/Activity pages.
- [ ] On Windows, verify the portable zip can launch `etle-gui.exe` without extra GTK installation.
