# Architecture

ETLE is an encrypted torrent-like file transfer application. It is not a BitTorrent implementation. It has its own protocol, descriptor, local state layout, daemon, CLI, GUI, and discovery mechanism.

## High-Level Runtime Model

```text
User
 ├─ etle-cli
 └─ etle-gui
      │
      │ IPC
      ▼
   etled
      ├─ local library state
      ├─ UDP discovery server
      └─ TCP P2P server/client transfer tasks
```

`etled` is the long-running local owner of the library. The CLI and GUI are clients that ask the daemon to seed, list, download, and shut down.

## Data Model

```text
Share
 ├─ ShareId
 ├─ EtleDescriptor
 ├─ Secret/file key
 ├─ encrypted chunks
 ├─ progress
 └─ reconstructed output
```

The public descriptor identifies what is being shared. The secret key is stored separately and must not be included in public metadata.

## Transfer Model

Seeder side:

```text
load local share
listen TCP
receive peer connection
hello/capabilities
X25519 key exchange
optional PSK auth proof
receive RequestShare
send manifest/descriptor data
send wrapped file key
send Have list
serve requested chunks
```

Downloader side:

```text
resolve peers manually or by discovery
connect to peer(s)
hello/capabilities
X25519 key exchange
optional PSK auth proof
request share
receive manifest
receive wrapped file key
receive Have list
request chunks
verify encrypted chunk hash
store chunk
decrypt/reconstruct when complete
```

## Module Layers

```text
src/bin/
  user-facing binary entry points

src/gui/
  GTK4/Relm4 frontend

src/ipc/
  local command/event channel between CLI/GUI and daemon

src/network/
  TCP, handshake, key exchange, transfer, workers

src/discovery/
  LAN UDP discovery query/response

src/protocol/
  wire messages and framing codec

src/state/
  local library paths, progress, share state

src/file/
  descriptor, manifest, package layout, chunk storage

src/crypto/
  AEAD, hashing, key exchange, key wrapping

src/config/
  config/env defaults
```

## Trust Boundaries

Trusted local state:

- `secret.etlekey`
- local progress/state files
- daemon IPC endpoint

Untrusted network input:

- UDP discovery packets
- TCP peer frames
- peer manifests/chunks
- remote advertised availability

Validation rules:

- Frames must have valid length and bincode payload.
- Descriptor/share IDs must match expected values.
- Chunk ciphertext must match BLAKE3 metadata.
- AEAD decrypt must succeed with correct AAD.
- Final reconstructed output must match expected hashes.
