# Code Map

This document maps major source files to their responsibilities.

## Binaries

### `src/bin/etled.rs`

Daemon entry point.

Responsibilities:

- Parse daemon CLI arguments.
- Load config/env defaults.
- Select library root.
- Start TCP P2P listener.
- Start UDP discovery server unless disabled.
- Start IPC listener unless disabled.
- Print startup banner.
- Pass PSK and logging settings into transfer/discovery layers.

### `src/bin/etle-cli.rs`

CLI client entry point.

Responsibilities:

- Parse CLI commands.
- Resolve IPC socket path.
- Send IPC commands to `etled`.
- Subscribe to IPC events for progress.
- Provide commands such as seed/list/download/ping/shutdown.
- Print build/version information.

### `src/bin/etle-gui.rs`

GUI entry point.

Responsibilities:

- Print version/build info when requested.
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
- share ID computation
- serialization/deserialization

### `src/file/package.rs`

Package layout and logical stream:

- collect single file layout
- collect directory layout
- deterministic file ordering
- global offsets
- logical stream chunking across files

### `src/file/manifest.rs`

Legacy/transfer manifest:

- file ID
- file name
- file size
- chunk size
- chunk metadata list

### `src/file/storage.rs`

Encrypted file/chunk helpers:

- encrypt file into chunks
- decrypt/reconstruct bytes
- debug workspace helpers

## Protocol

### `src/protocol/message.rs`

Wire message enum and protocol constants.

Important messages:

- `Hello`
- `Capabilities`
- `KeyExchange`
- `AuthProof`
- `RequestShare`
- `Manifest`
- `WrappedFileKey`
- `Have`
- `RequestChunk`
- `Chunk`
- `Error`

### `src/protocol/codec.rs`

Async frame codec:

- send message
- receive message
- length prefix validation
- raw chunk frame handling
- bincode trailing-byte checks

## Discovery

### `src/discovery/options.rs`

Discovery runtime options:

- UDP port
- timeout
- multicast address
- verbose logging flag

### `src/discovery/client.rs`

Discovery query sender and response collector.

### `src/discovery/server.rs`

Discovery query responder owned by the daemon.

### `src/discovery/network.rs`

Discovery network helpers:

- interface enumeration
- broadcast/multicast target generation
- local share lookup
- advertised address handling
- instance ID/dedup helpers

## State

### `src/state/paths.rs`

Default ETLE library root and path helpers.

### `src/state/library.rs`

Local share library operations:

- create/read share records
- list shares
- resolve descriptor/secret/chunk paths
- count completed chunks

### `src/state/progress.rs`

Progress persistence and missing/completed chunk tracking.

## Network Transfer

### `src/network/tcp.rs`

TCP bind/connect helpers.

### `src/network/handshake.rs`

Hello/capability handshake flow.

### `src/network/key_exchange.rs`

Network session key exchange and PSK-authenticated variant.

### `src/network/transfer/serve.rs`

Seeder-side peer session handling.

### `src/network/transfer/download.rs`

Downloader-side transfer handling.

### `src/network/transfer/options.rs`

Transfer options:

- log level
- request window
- parallelism
- library root
- PSK

### `src/network/transfer/jobs.rs`

In-process active job registry.

## IPC

### `src/ipc/message.rs`

IPC command/response/event types.

### `src/ipc/server/`

Daemon-side IPC command handling.

### `src/ipc/client.rs`

CLI/GUI-side IPC client helpers.

### `src/ipc/path.rs`

Default IPC path resolution.
