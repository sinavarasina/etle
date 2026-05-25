# Development

## Local Commands

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
```

GUI build:

```bash
cargo build --locked --release --no-default-features --features gui-relm4 --bin etle-gui
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

## Code Style

- Prefer explicit errors over silent failure.
- Keep network input validation strict.
- Keep noisy logs behind verbose mode.
- Keep release workflow deterministic and easy to inspect.
- Keep `README.md` high-level and put detailed internals in `docs/`.

## Testing Areas

Important tests to keep or expand:

- crypto roundtrip
- wrong nonce/AAD/ciphertext failure
- descriptor deterministic share ID
- protocol frame rejection
- discovery local seeder test
- IPC command tests
- resume/progress tests
- parallel download tests
- GUI build checks
