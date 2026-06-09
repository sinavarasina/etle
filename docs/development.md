# Development

## Local Commands

Quick checks:

```bash
cargo fmt
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Preferred stricter checks:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
```

GUI check:

```bash
cargo check --locked --no-default-features --features gui-relm4 --bin etle-gui
cargo clippy --locked --no-default-features --features gui-relm4 --bin etle-gui -- -D warnings
```

GUI build:

```bash
cargo build --locked --release --no-default-features --features gui-relm4 --bin etle-gui
```

Benchmark compile check:

```bash
cargo bench --no-run
```

## Running a Local Demo

Terminal 1:

```bash
cargo run --bin etled -- -v serve
```

Terminal 2:

```bash
cargo run --bin etle-cli -- seed ./sample.bin
cargo run --bin etle-cli -- list
cargo run --bin etle-cli -- download --share-id <64_HEX_SHARE_ID>
```

Manual peer:

```bash
cargo run --bin etle-cli -- download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 127.0.0.1:7000
```

Delete a test share:

```bash
cargo run --bin etle-cli -- delete --share-id <64_HEX_SHARE_ID>
```

GUI demo:

```bash
cargo run --no-default-features --features gui-relm4 --bin etle-gui
```

## Logging

Use `-v` for verbose daemon logs:

```bash
etled -v serve
```

Verbose logs are especially useful for:

- discovery packets
- transfer session setup
- key exchange
- chunk progress
- peer failures
- destructive local library actions

## Code Style

- Prefer explicit errors over silent failure.
- Keep network input validation strict.
- Keep noisy logs behind verbose mode unless the operation is destructive or critical.
- Keep release workflow deterministic and easy to inspect.
- Keep `README.md` high-level and put detailed internals in `docs/`.
- Keep CLI/GUI/daemon IPC variants documented when they change.
- Keep platform-specific GUI styling isolated under `src/gui/style/`.

## Testing Areas

Important tests to keep or expand:

- crypto roundtrip
- wrong nonce/AAD/ciphertext failure
- descriptor deterministic share ID
- protocol frame rejection
- discovery local seeder test
- IPC command tests, including share deletion
- resume/progress tests
- partial seeder tests
- parallel download tests
- GUI build checks
- Windows named-pipe/default-path checks
