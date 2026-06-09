# Transfer Flow

This document describes the full transfer flow from seeder to downloader.

## Actors

```text
Seeder: has descriptor, secret, and chunks.
Downloader: wants a share_id.
Daemon: owns local library state and worker tasks.
CLI/GUI: sends IPC commands to daemon.
```

## Seeder Startup

```text
1. User starts etled serve.
2. Daemon resolves library root.
3. Daemon loads local shares.
4. Daemon prints share inventory.
5. Daemon binds TCP listener.
6. Daemon starts discovery server.
7. Daemon starts IPC listener.
8. Daemon publishes ServerStarted event.
```

## Download Startup

```text
1. User requests download through CLI or GUI.
2. CLI/GUI sends IPC command to daemon.
3. Daemon resolves share_id and options.
4. Daemon builds peer candidate list:
   - manual peers
   - discovery results
5. Daemon starts transfer job.
6. Daemon emits queued/progress events.
```

## Session Setup

```text
1. Downloader opens TCP connection to peer.
2. Both sides exchange Hello.
3. Both sides exchange capabilities.
4. Both sides exchange X25519 public keys.
5. Both derive transcript-bound session key.
6. If PSK is configured:
   a. client sends auth proof
   b. server verifies
   c. server sends auth proof
   d. client verifies
7. Session is ready.
```

## Share Negotiation

```text
1. Downloader sends RequestShare { share_id }.
2. Seeder checks local library.
3. Seeder rejects if share is unknown/unavailable.
4. Seeder sends manifest/descriptor data.
5. Seeder wraps the file key with session key.
6. Seeder sends WrappedFileKey.
7. Seeder sends Have list.
```

## Chunk Transfer

```text
1. Downloader computes missing chunks.
2. Downloader selects chunk and peer.
3. Downloader sends RequestChunk { index }.
4. Seeder checks availability.
5. Seeder reads encrypted chunk.
6. Seeder sends Chunk { index, data } or raw chunk frame.
7. Downloader hashes encrypted data with BLAKE3.
8. Downloader compares with metadata.
9. If valid:
   a. write chunk to local chunk store
   b. mark progress complete
   c. emit progress event
10. If invalid:
   a. reject chunk
   b. retry from same/other peer according to policy
```

## Completion

```text
1. All chunks are complete.
2. Downloader unwraps/loads file key.
3. Downloader decrypts chunks in order.
4. Downloader reconstructs file/folder output.
5. Downloader verifies output hashes.
6. Share becomes completed/seedable.
7. IPC event reports completion.
8. CLI/GUI displays success.
```

Local share deletion is not part of the network transfer protocol. It is a daemon IPC operation that removes local share state after user confirmation in CLI/GUI flows.

## Error Handling

Common fatal errors:

- no peers available
- all peers fail
- share ID mismatch
- auth proof mismatch
- wrapped key decrypt failed
- chunk verification failed after retry limit
- missing chunk with no provider
- reconstruction hash mismatch
- requested local share no longer exists

Common non-fatal errors:

- one peer disconnects while other peers exist
- one chunk request fails and is retried
- discovery response cannot be decoded
- discovery finds duplicate peer
- GUI event subscription disconnects and reconnects
