# ETLE examples

These examples are intentionally local and deterministic enough for CI. They do
not open TCP sockets and do not require a running `etled` daemon.

## `local_roundtrip`

Runs the modern local crypto path:

1. X25519 ephemeral key exchange
2. transcript-bound session key
3. PSK proof check
4. random file key generation
5. file-key wrapping/unwrapping
6. chunk encryption/decryption
7. final hash verification

```sh
cargo run --example local_roundtrip -- ./sample.bin
```

Optional PSK override:

```sh
ETLE_EXAMPLE_PSK='test shared secret' \
  cargo run --example local_roundtrip -- ./sample.bin
```

## `debug_roundtrip`

Same core flow, but writes a debug workspace containing the manifest and encrypted
chunks:

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
