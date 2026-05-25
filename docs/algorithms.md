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
3. Allocate buffer of chunk_size.
4. Read up to chunk_size bytes.
5. If read == 0, stop.
6. Truncate buffer to actual bytes read.
7. Emit PlainChunk { index, data }.
8. Increment index.
9. Repeat.
```

Properties:

- Chunk indices start from 0.
- Last chunk may be smaller than `chunk_size`.
- If index overflows `u32`, reject as too many chunks.

## 4. Directory Package Layout Algorithm

Purpose:

- Convert a folder into a deterministic logical package stream.

Algorithm:

```text
input: directory path
output: PackageLayout

1. Recursively walk the directory.
2. Collect only regular files.
3. Sort collected paths lexicographically.
4. Reject empty package.
5. For each file:
   a. Compute relative path from package root.
   b. Normalize separator to "/".
   c. Read file size.
   d. Compute FileId = BLAKE3(file bytes).
   e. Assign current global offset.
   f. Advance global offset by file size.
6. Return PackageLayout {
     name,
     root_path,
     total_size,
     files
   }.
```

Properties:

- File ordering is deterministic.
- Offsets do not overlap.
- Multi-file package is treated as one logical byte stream.

## 5. Package Stream Chunking Algorithm

Purpose:

- Split a multi-file package into chunks without losing file boundaries in metadata.

Algorithm:

```text
input: PackageLayout, chunk_size
output: Vec<PlainChunk>

1. Reject chunk_size == 0.
2. Create empty chunk_buffer.
3. For each source file in package order:
   a. Open file.
   b. Read file in smaller I/O blocks.
   c. Append read bytes into chunk_buffer.
   d. Whenever chunk_buffer reaches chunk_size:
      - emit PlainChunk { index, data }
      - clear buffer
      - increment index
4. After all files, emit final chunk if buffer is not empty.
```

Properties:

- Chunks may cross file boundaries.
- The descriptor `FileEntry` list is required to reconstruct files.
- Chunking is stable if file order and chunk size are stable.

## 6. Share ID Algorithm

Purpose:

- Create a stable identity for a share/package.

Algorithm shape:

```text
input:
  descriptor_version
  package name
  total_size
  chunk_size
  crypto suite
  file entries
  chunk metadata

output:
  ShareId = BLAKE3(canonical descriptor fields)
```

Canonical hash feed:

```text
domain separator: "etle descriptor share id v1"
version as little-endian
name length + name bytes
total_size as little-endian
chunk_size as little-endian
crypto suite domain tag
file count
for each file:
  path length + path bytes
  size
  offset
  file BLAKE3 hash
chunk count
for each chunk:
  index
  plain_size
  encrypted_size
  nonce
  encrypted chunk BLAKE3 hash
```

Properties:

- Same package metadata produces the same share ID.
- Changing path, size, offset, file hash, chunk hash, nonce, crypto suite, or chunk size changes the share ID.
- `ShareId` identifies the encrypted share metadata, not just one file.

## 7. Chunk Encryption Algorithm

Purpose:

- Encrypt chunks with reusable ciphertext.

Algorithm:

```text
input:
  file_key
  file_id/share context
  chunk_index
  plain_size
  plaintext chunk

output:
  nonce
  ciphertext
  chunk hash metadata

1. Generate random 24-byte XChaCha nonce.
2. Build AAD from identity + chunk_index + plain_size.
3. Encrypt plaintext using XChaCha20-Poly1305(file_key, nonce, AAD).
4. Hash ciphertext with BLAKE3.
5. Store ChunkMeta {
     index,
     plain_size,
     encrypted_size,
     nonce,
     blake3_hash
   }.
6. Store ciphertext in chunk storage.
```

Properties:

- Wrong nonce fails decrypt.
- Wrong AAD fails decrypt.
- Tampered ciphertext fails decrypt.
- Ciphertext is reusable because it depends on share file key, not peer session key.

## 8. Key Exchange Algorithm

Purpose:

- Establish a session key between two peers.

Unauthenticated algorithm:

```text
client:
  generate ephemeral X25519 keypair
  send client_public

server:
  generate ephemeral X25519 keypair
  send server_public

both:
  shared_secret = X25519(own_secret, peer_public)
  reject all-zero shared secret
  session_key = BLAKE3 derive_key(shared_secret + client_public + server_public)
```

Properties:

- Passive observers cannot derive the session key.
- Without authentication, active MITM is still possible.

## 9. PSK Authentication Algorithm

Purpose:

- Add simple shared-secret authentication to the X25519 transcript.

Algorithm:

```text
input:
  PSK
  session_key
  client_public
  server_public
  role

output:
  auth_tag

1. Convert passphrase to 32-byte AuthPsk using BLAKE3 derive_key.
2. Create keyed BLAKE3 hasher using AuthPsk.
3. Feed domain separator.
4. Feed role: "client" or "server".
5. Feed session_key.
6. Feed client_public.
7. Feed server_public.
8. Finalize to 32-byte tag.
9. Compare received tag using constant-time equality.
```

Properties:

- Wrong PSK fails authentication.
- Public-key swap changes the transcript and fails authentication.
- Roles prevent reflected proof confusion.

## 10. File Key Wrapping Algorithm

Purpose:

- Send the reusable file key through a session without exposing it in plaintext.

Algorithm:

```text
input:
  session_key
  share/file identity
  file_key

output:
  WrappedFileKey { nonce, data }

1. Generate random XChaCha nonce.
2. Build key-wrap AAD with domain separator and identity.
3. Encrypt file_key bytes using session_key.
4. Send WrappedFileKey.
```

Unwrap:

```text
1. Build same AAD.
2. Decrypt wrapped bytes using session_key.
3. Require exactly 32 bytes.
4. Convert to SymmetricKey.
```

Properties:

- Wrong session key fails.
- Wrong identity/AAD fails.
- Wrapped key does not reveal file key without session key.

## 11. Discovery Algorithm

Purpose:

- Find LAN peers that can serve a known share.

Client:

```text
input: share_id, discovery targets, timeout
output: peer addresses

1. Build DiscoveryMessage::Query { magic, share_id }.
2. Send query to loopback/broadcast/multicast targets.
3. Receive responses until timeout.
4. Decode each response.
5. Reject bad magic or wrong share_id.
6. Resolve 0.0.0.0 listen_addr using UDP source IP.
7. Deduplicate peers by instance_id/address.
8. Return discovered peer list.
```

Server:

```text
1. Bind UDP 0.0.0.0:discovery_port.
2. Enable broadcast.
3. Join multicast on active interfaces.
4. Receive packet.
5. Decode query.
6. Reject bad magic/non-query.
7. Look up share_id in local library.
8. If local share is discoverable, send Response.
```

## 12. Download Algorithm

Purpose:

- Download and verify all chunks for a share.

Algorithm:

```text
input:
  share_id
  peer list/discovery settings
  output path
  parallelism
  request window
  optional PSK

1. Resolve candidate peers:
   a. manual peers
   b. discovered peers
2. Load existing local library progress if available.
3. Compute missing chunk list.
4. Connect to candidate peers.
5. Run handshake/key exchange/auth if configured.
6. Request share.
7. Receive manifest/descriptor.
8. Receive wrapped file key.
9. Receive Have list.
10. Schedule missing chunks.
11. For each chunk:
    a. choose a peer that has it
    b. request chunk
    c. verify BLAKE3 ciphertext hash
    d. persist encrypted chunk
    e. mark progress complete
12. When all chunks are complete:
    a. decrypt/reconstruct output
    b. verify final file/file-entry hashes
    c. mark share completed/seedable
```

## 13. Parallel Worker Algorithm

Purpose:

- Download multiple chunks concurrently.

Algorithm shape:

```text
input: missing_chunks, peer_availability, parallelism

1. Create shared queue of missing chunks.
2. Spawn N workers.
3. Each worker:
   a. pick next chunk not already in progress
   b. choose peer with that chunk
   c. request chunk
   d. verify and store
   e. mark completed
   f. on failure, requeue chunk with retry/backoff
4. Stop when all chunks complete or no peer can serve remaining chunks.
```

Safety rules:

- Do not write an unverified chunk to final progress.
- Do not mark progress complete before BLAKE3 verification.
- Do not let two workers finalize the same chunk concurrently.
- Treat peer failure as chunk retry, not entire download failure when alternatives exist.

## 14. Reconstruction Algorithm

Single logical stream to output:

```text
1. Load descriptor file entries.
2. Open output file(s).
3. Iterate chunks in order.
4. Decrypt chunk using file_key and AAD.
5. Write bytes into logical stream.
6. Split bytes according to FileEntry offset/size.
7. After each file, verify file BLAKE3 hash.
8. Mark reconstruction success only if every file hash matches.
```

Properties:

- Multi-file output requires descriptor file offsets.
- Final validation must be per file or equivalent full logical stream validation.
- Reconstruct should fail closed on missing/corrupt chunks.

## 15. Release Checksum Algorithm

Per-package binary checksums:

```text
1. After copying binaries into package directory.
2. Run SHA-256 over each binary.
3. Write SHA256SUMS.txt inside package.
```

Release archive checksums:

```text
1. After creating .tar.gz/.zip assets.
2. Run SHA-256 over each archive.
3. Write CHECKSUMS.txt as release asset.
```

A binary cannot contain its own final checksum because embedding that checksum changes the binary itself.
