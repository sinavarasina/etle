# Data Formats

This document describes ETLE data structures and binary files conceptually.

The exact serialization currently uses Rust structs and bincode. The public format is not stable yet.

## ShareId

```rust
pub struct ShareId(pub [u8; 32]);
```

Display format:

```text
64 lowercase hex characters
```

Example:

```text
883686bf2a7331c5cc5e07b249da7f577246cd076762233ee65519a1fa5dfe9c
```

## FileId

```rust
pub struct FileId(pub [u8; 32]);
```

Meaning:

```text
BLAKE3(file bytes)
```

## ChunkHash

```rust
pub struct ChunkHash(pub [u8; 32]);
```

Meaning:

```text
BLAKE3(encrypted chunk bytes)
```

## Nonce

```rust
pub struct Nonce(pub [u8; 24]);
```

Used for XChaCha20-Poly1305.

## SymmetricKey

```rust
pub struct SymmetricKey(pub [u8; 32]);
```

Debug output must redact the key.

## ChunkMeta

```rust
pub struct ChunkMeta {
    pub index: u32,
    pub plain_size: u64,
    pub encrypted_size: u64,
    pub nonce: Nonce,
    pub blake3_hash: ChunkHash,
}
```

Meaning:

- `index`: global chunk number.
- `plain_size`: plaintext bytes before encryption.
- `encrypted_size`: ciphertext bytes after encryption.
- `nonce`: XChaCha nonce for this chunk.
- `blake3_hash`: hash of encrypted bytes.

## FileEntry

```rust
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub offset: u64,
    pub blake3_hash: FileId,
}
```

Meaning:

- `path`: relative path inside package, using `/`.
- `size`: file size in bytes.
- `offset`: offset in logical package stream.
- `blake3_hash`: hash of original file bytes.

## EtleDescriptor

```rust
pub struct EtleDescriptor {
    pub version: u16,
    pub name: String,
    pub share_id: ShareId,
    pub total_size: u64,
    pub chunk_size: u64,
    pub crypto: CryptoSuite,
    pub files: Vec<FileEntry>,
    pub chunks: Vec<ChunkMeta>,
}
```

Meaning:

- Public share metadata.
- Enough to verify and reconstruct a share.
- Does not contain the file key.

## CryptoSuite

Current suite:

```rust
XChaCha20Poly1305Blake3X25519V1
```

Meaning:

- chunk encryption: XChaCha20-Poly1305
- hashing: BLAKE3
- session key exchange: X25519

## WrappedFileKey

```rust
pub struct WrappedFileKey {
    pub nonce: Nonce,
    pub data: Vec<u8>,
}
```

Meaning:

- `data` is encrypted file key bytes.
- Decrypted plaintext must be exactly 32 bytes.
- AAD binds the wrapped key to the share/file identity.

## Local Library Files

### `descriptor.etle`

Serialized `EtleDescriptor`.

### `secret.etlekey`

Local secret state for decrypting/serving share chunks.

### `progress.bin`

Serialized progress information.

### `state.bin`

Serialized local state.

### `chunks/<index>.etle`

Encrypted chunk bytes.

## Stability Warning

These files are project-internal and may change until ETLE declares a stable format versioning policy.
