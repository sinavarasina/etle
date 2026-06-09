# ETLE examples

These examples are local and deterministic enough for CI. They do not open TCP
sockets and do not require a running `etled` daemon.

Example output uses a compact `key=value` format so it is easy to compare in
logs across Linux, macOS, and Windows.

## `local_roundtrip`

Runs the local authenticated encrypted roundtrip:

1. X25519 ephemeral key exchange
2. transcript-bound session key derivation
3. PSK proof verification
4. file-key generation and wrapping
5. chunk encryption/decryption
6. final hash verification

```sh
cargo run --example local_roundtrip -- ./sample.bin
```

Optional PSK override:

```sh
ETLE_EXAMPLE_PSK='test shared secret' \
  cargo run --example local_roundtrip -- ./sample.bin
```

## `debug_roundtrip`

Runs the same core flow, then writes a debug workspace containing the manifest,
encrypted chunks, and reconstructed output:

```sh
cargo run --example debug_roundtrip -- ./sample.bin .etle-work/sample
```

The workspace is useful when inspecting chunk metadata, encrypted chunk sizes, or
reconstruction behavior.

## GUI build

The GUI is feature-gated. Build it without default CLI features when you only
want the GUI binary:

```sh
cargo run --no-default-features --features gui-relm4 --bin etle-gui
```
