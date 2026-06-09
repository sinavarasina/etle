# Crypto Design

ETLE uses modern primitives, but the project is experimental and not independently audited.

## Primitives

- Hashing: BLAKE3
- Chunk encryption: XChaCha20-Poly1305
- Key exchange: X25519
- Optional authentication: PSK-derived BLAKE3 keyed auth tags
- Serialization of metadata/wire structures: `bincode-next` with explicit trailing-byte checks

## Hash Types

`FileId` and `ChunkHash` are 32-byte BLAKE3 outputs.

Use cases:

- file identity
- chunk ciphertext verification
- descriptor/share identity input
- final reconstruction verification

`ShareId` is also derived from descriptor metadata with BLAKE3, but it represents the whole share descriptor rather than a single file or chunk.

## Chunk Encryption

Each encrypted chunk uses:

- a 32-byte symmetric file key
- a 24-byte XChaCha nonce
- AAD binding chunk metadata

Chunk AAD contains:

```text
file/share identity
chunk index
plain size
```

If any AAD field changes, AEAD decryption fails.

## Reusable File Key

Torrent-like reuse needs stable ciphertext. If every peer session produced a different encryption key, chunks would not be reusable between peers.

ETLE therefore uses:

```text
random file_key per share
file_key encrypts chunks
session_key wraps file_key for a peer session
```

The descriptor is public and does not include the file key. The local file key is stored in `secret.etlekey` and must be protected.

## X25519 Session

Peers exchange ephemeral public keys and derive a session key.

Unauthenticated mode:

```text
X25519 shared secret
+ transcript
→ session key
```

Authenticated PSK mode:

```text
X25519 shared secret
+ transcript
→ session key

PSK
+ session key
+ transcript
+ role
→ auth proof
```

Both sides must use the same PSK. If the PSK is wrong or missing on one side, the authenticated session fails.

## Security Limitations

- Unauthenticated X25519 is not MITM-resistant.
- PSK security depends on the passphrase strength and secrecy.
- No public key identity system exists yet.
- No formal protocol audit has been done.
- No NAT traversal or anti-traffic-analysis design exists.
- Local `secret.etlekey` must be protected by filesystem permissions.
- Local share deletion removes local encrypted chunks and secret state; it is not secure erasure.
