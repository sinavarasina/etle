# Code Map

This document maps major source files to their responsibilities.

## Binaries

### `src/bin/etled.rs`

Daemon entry point.

Responsibilities:

- Parse daemon CLI arguments.
- Load config/env defaults.
- Select library root and IPC endpoint.
- Start TCP P2P listener.
- Start UDP discovery server unless disabled.
- Start IPC listener unless disabled.
- Print startup banner and supported IPC commands.
- Pass PSK and logging settings into transfer/discovery layers.
- Publish server start/stop events.

### `src/bin/etle-cli.rs`

CLI client entry point.

Responsibilities:

- Parse CLI commands.
- Resolve IPC socket or Windows named pipe path.
- Send IPC commands to `etled`.
- Subscribe to IPC events for progress.
- Provide commands such as seed/list/delete/download/ping/shutdown/watch.
- Print build/version information.

### `src/bin/etle-gui.rs`

GUI entry point.

Responsibilities:

- Print version/build info when requested.
- Configure platform environment before GTK/Relm4 starts.
- Run the GTK4/Relm4 GUI when `gui-relm4` is enabled.

## Crypto

### `src/crypto/hash.rs`

BLAKE3 helpers and hash newtypes:

- `FileId`
- `ChunkHash`
- byte hashing
- file hashing
- hex display

### `src/crypto/aead.rs`

XChaCha20-Poly1305 helpers:

- `SymmetricKey`
- `Nonce`
- random nonce generation
- chunk AAD construction
- encrypt/decrypt helpers

### `src/crypto/key_exchange.rs`

X25519 and authentication helpers:

- ephemeral keypair
- public key bytes
- shared secret
- session key derivation
- transcript-bound derivation
- PSK auth tag generation
- constant-time auth tag comparison

### `src/crypto/key_wrap.rs`

Reusable file key helpers:

- random file key generation
- wrap file key with session key
- unwrap file key
- key-wrap AAD construction

## File and Descriptor

### `src/file/descriptor.rs`

Share descriptor model:

- `ShareId`
- `EtleDescriptor`
- `FileEntry`
- `CryptoSuite`
- deterministic share ID computation
- `bincode-next` serialization/deserialization

### `src/file/package.rs`

Package layout for files and directories:

- single-file layout
- directory traversal
- stable file ordering
- logical package offsets
- package stream chunking

### `src/file/manifest.rs`

Transfer manifest model and serialization.

### `src/file/storage.rs`

File encryption/decryption and debug workspace helpers.

## Protocol

### `src/protocol/message.rs`

Wire message enum and capability constants.

### `src/protocol/codec.rs`

Frame codec:

- frame size prefix
- frame size limits
- `bincode-next` encode/decode
- raw chunk frame support

## Discovery

### `src/discovery/options.rs`

Discovery port, timeout, multicast, and broadcast options.

### `src/discovery/client.rs`

Discovery query client and peer collection.

### `src/discovery/server.rs`

Discovery response server tied to local library state.

### `src/discovery/network.rs`

Discovery target calculation, response address handling, local share filtering, and encode/decode helpers.

## State

### `src/state/paths.rs`

Default library root and per-share path helpers.

### `src/state/library.rs`

Local library operations:

- list shares
- initialize share state
- delete a share directory by `ShareId`

### `src/state/storage.rs`

Read/write helpers for descriptor, secret, progress, state, and chunks.

## Network Transfer

### `src/network/tcp.rs`

TCP bind/connect helpers.

### `src/network/handshake.rs`

Protocol hello/capability handshake.

### `src/network/key_exchange.rs`

Network-level session key exchange, including PSK-authenticated mode.

### `src/network/transfer/serve.rs`

Seeder/library serving paths.

### `src/network/transfer/download.rs`

Single-peer, multi-peer, discovery-assisted, resume, and parallel download paths.

### `src/network/transfer/options.rs`

Transfer options and defaults.

### `src/network/transfer/jobs.rs`

Active transfer job registry.

### `src/network/transfer/progress.rs`

Progress event/log helpers.

## IPC

### `src/ipc/message.rs`

IPC command, response, and event types, including:

- `DeleteShare`
- `ShareDeleted`
- transfer queued/completed events
- server started/stopped events

### `src/ipc/server/`

Daemon IPC listener, command dispatch, cleanup, and event fanout.

### `src/ipc/client.rs`

CLI/GUI IPC client helpers.

### `src/ipc/path.rs`

Default IPC endpoint selection:

- Unix-like: `<library-root>/.etle/etled.sock`
- Windows: `\\.\pipe\etled`

## GUI

### `src/gui/app.rs`

Relm4 application state/update logic.

### `src/gui/widgets.rs`

GTK widget construction and list refill helpers.

### `src/gui/style.rs`

Platform style/environment installer.

### `src/gui/style/windows.css`

Windows-only app-scoped GTK CSS for a Fluent-like appearance.
