# Algorithms

This document explains the important algorithms used by ETLE at a code/design level.

## 1. File Hashing Algorithm

Purpose:

- Identify file content.
- Verify reconstructed output.
- Feed descriptor/share identity.

Algorithm:

```text
input: file path
output: FileId = BLAKE3(file bytes)

1. Open file.
2. Create BLAKE3 hasher.
3. Read file in 64 KiB blocks.
4. Feed every block into hasher.
5. Finalize hasher.
6. Store 32-byte digest as FileId.
```

Properties:

- Same file bytes produce the same `FileId`.
- Any byte change changes the hash.
- File name is not part of `FileId`; only bytes are.

## 2. Chunk Hashing Algorithm

Purpose:

- Verify encrypted chunks before storing/using them.
- Detect corrupted network payloads.
- Allow reusable chunks between peers.

Algorithm:

```text
input: encrypted chunk bytes
output: ChunkHash = BLAKE3(encrypted chunk bytes)

1. Receive or generate encrypted chunk.
2. Hash ciphertext bytes with BLAKE3.
3. Compare with descriptor/manifest chunk hash.
4. Accept chunk only if equal.
```

Important detail:

ETLE hashes encrypted chunk bytes, not plaintext chunk bytes, for transfer/storage verification. Plaintext validation happens later through AEAD decrypt and final file hashes.

## 3. File Chunking Algorithm

Purpose:

- Split a file into deterministic fixed-size chunks.

Algorithm:

```text
input: file path, chunk_size
output: Vec<PlainChunk>

1. Reject chunk_size == 0.
2. Open file.
3. Read at most chunk_size bytes.
4. Emit PlainChunk { index, data }.
5. Increment index.
6. Repeat until EOF.
```

## 4. Directory Package Layout Algorithm

Purpose:

- Represent a folder as one logical share.

Algorithm:

```text
input: directory root
output: PackageLayout

1. Recursively collect regular files.
2. Sort paths for stable order.
3. Convert every path to a '/' relative path.
4. Hash every file.
5. Assign logical offsets in sorted order.
6. Store FileEntry for each file.
```

## 5. Package Stream Chunking Algorithm

Purpose:

- Chunk a multi-file package as one logical byte stream.

Algorithm:

```text
1. Iterate files in PackageLayout order.
2. Read file bytes into a shared chunk buffer.
3. Emit a chunk whenever the buffer reaches chunk_size.
4. Continue across file boundaries.
5. Emit final partial chunk if non-empty.
```

## 6. Share ID Algorithm

Purpose:

- Bind public descriptor metadata into one stable ID.

Algorithm:

```text
1. Hash domain tag.
2. Hash descriptor version.
3. Hash share name, total size, chunk size, crypto suite.
4. Hash every FileEntry in stable order.
5. Hash every ChunkMeta in stable order.
6. Finalize BLAKE3 output as ShareId.
```

## 7. Chunk Encryption Algorithm

Purpose:

- Encrypt each chunk independently while binding metadata.

Algorithm:

```text
1. Generate random XChaCha nonce.
2. Build AAD from file ID, chunk index, and plain size.
3. Encrypt plaintext chunk with file key.
4. Hash ciphertext bytes.
5. Store ChunkMeta and encrypted chunk.
```

## 8. Key Exchange Algorithm

Purpose:

- Derive a session key for wrapping the file key.

Algorithm:

```text
1. Client generates X25519 ephemeral keypair.
2. Server generates X25519 ephemeral keypair.
3. Exchange public keys.
4. Compute shared secret.
5. Reject all-zero shared secret.
6. Derive transcript-bound session key.
```

## 9. PSK Authentication Algorithm

Purpose:

- Authenticate the transcript against an out-of-band passphrase.

Algorithm:

```text
1. Derive AuthPsk from passphrase.
2. Build keyed BLAKE3 hasher with PSK bytes.
3. Hash domain tag.
4. Hash role, session key, client public key, server public key.
5. Compare expected tag in constant-time style.
```

## 10. File Key Wrapping Algorithm

Purpose:

- Send the reusable file key to a peer without exposing it on the wire.

Algorithm:

```text
1. Build AAD from key-wrap domain tag and file ID.
2. Encrypt file key with session key.
3. Send WrappedFileKey { nonce, data }.
4. Peer decrypts and checks plaintext is 32 bytes.
```

## 11. Discovery Algorithm

Purpose:

- Find LAN peers that can serve a known share.

Algorithm:

```text
1. Build query from discovery magic and ShareId.
2. Send query to loopback, broadcast, and multicast targets.
3. Seeder checks local library for share.
4. Seeder responds with listen address, port, peer ID, instance ID, and name.
5. Client deduplicates by instance ID.
6. If response listen address is unspecified, use UDP source IP.
```

## 12. Download Algorithm

Purpose:

- Download, verify, persist, and reconstruct a share.

Algorithm:

```text
1. Resolve peers manually or by discovery.
2. Connect and authenticate session.
3. Request share.
4. Receive manifest, wrapped file key, and Have list.
5. Load local progress.
6. Request missing chunks.
7. Verify and store chunks.
8. Reconstruct when complete.
9. Emit IPC progress/completion events.
```

## 13. Parallel Worker Algorithm

Purpose:

- Fetch chunks from multiple peers concurrently.

Algorithm:

```text
1. Build missing chunk queue.
2. Build chunk availability map.
3. Spawn worker count from --parallel.
4. Each worker chooses available chunks and peers.
5. Failed chunks are requeued when possible.
6. Completed chunks update shared progress.
```

## 14. Reconstruction Algorithm

Purpose:

- Restore original file/folder bytes.

Algorithm:

```text
1. Read descriptor and local secret.
2. Read encrypted chunks in index order.
3. Verify ciphertext hash for every chunk.
4. AEAD decrypt with expected AAD.
5. Join plaintext chunks.
6. Split output according to FileEntry offsets/sizes when needed.
7. Verify final file hashes.
```

## 15. Release Checksum Algorithm

Purpose:

- Make release packages inspectable.

Algorithm:

```text
1. Build binaries with release metadata.
2. Copy binaries into package directory.
3. Generate SHA256SUMS.txt inside package.
4. Archive package.
5. Generate top-level CHECKSUMS.txt for archives.
```

## 16. Local Share Deletion Algorithm

Purpose:

- Remove one daemon-owned local share from the library.

Algorithm:

```text
input: library root, ShareId
output: deleted true/false or error

1. Resolve LibraryPaths from library root and ShareId.
2. Locate .etle/library/<share_id>/.
3. If missing, return false.
4. Remove the share directory recursively.
5. Publish ShareDeleted IPC event.
6. CLI/GUI refresh local share list.
```

Safety property:

Deletion is based on a parsed `ShareId` and `LibraryPaths`, not a user-provided arbitrary filesystem path.
