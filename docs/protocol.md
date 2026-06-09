# Wire Protocol

ETLE uses a custom TCP protocol. It is not BitTorrent-compatible.

## Framing

Most protocol messages use:

```text
u32 big-endian frame length
bincode-next payload
```

The codec rejects:

- empty frames
- oversized frames
- trailing bytes after decode
- invalid `bincode-next` payloads

The protocol also has a raw chunk frame capability for efficient chunk transfer.

## Protocol Version

The protocol exposes:

```text
ETLE_WIRE_PROTOCOL_VERSION
CAPABILITY_RAW_CHUNK_FRAME
CAPABILITY_WINDOWED_REQUESTS
```

Peers exchange capability information early in the session.

## Messages

Current message set:

```rust
Hello { peer_id }
Capabilities { protocol_version, features }
KeyExchange { public_key }
AuthProof { tag }
RequestManifest
RequestShare { share_id }
Manifest { manifest }
WrappedFileKey { nonce, data }
Have { chunks }
RequestChunk { index }
Chunk { index, data }
Error { message }
```

IPC has its own command/event model and is not the same as the P2P wire message set.

## Typical Flow

```text
client -> server: Hello
server -> client: Hello

client <-> server: Capabilities

client -> server: KeyExchange(client_public)
server -> client: KeyExchange(server_public)

optional:
client -> server: AuthProof(client_tag)
server -> client: AuthProof(server_tag)

client -> server: RequestShare(share_id)

server -> client: Manifest(...)
server -> client: WrappedFileKey(...)
server -> client: Have([...])

client -> server: RequestChunk(index)
server -> client: Chunk(index, encrypted_data)
```

With raw chunk frame support, the chunk payload may be sent through the optimized raw chunk path instead of the normal enum payload path.

## Validation

The downloader must validate:

1. The server responds for the requested `share_id`.
2. The manifest/descriptor matches the expected share.
3. Wrapped key decrypts successfully.
4. Each chunk hash matches metadata.
5. AEAD decrypt succeeds with expected AAD.
6. Reconstructed file/folder hash matches metadata.

The decoder must validate:

1. Frame size is non-zero and within the limit.
2. Decode consumes the whole frame.
3. Unexpected message kinds fail closed.

## Compatibility Policy

The wire protocol is not stable yet. Any incompatible change should bump `ETLE_WIRE_PROTOCOL_VERSION` and be documented in release notes.

IPC compatibility is also not stable yet. CLI, GUI, and daemon should be updated together when IPC command/response/event variants change.
