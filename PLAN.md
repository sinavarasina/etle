# ETLE PLAN

This plan is written as sprint-only project tracking. It replaces the older phase/task-heavy plan with a cleaner roadmap that matches the current codebase direction.

## Sprint 1 — Crypto, Descriptor, and Package Foundation

Status: complete.

Goal:

- Build the cryptographic and file metadata base needed for reusable encrypted chunks.

Done:

- [x] BLAKE3 byte/file/chunk hashing.
- [x] `FileId` and `ChunkHash`.
- [x] XChaCha20-Poly1305 chunk encryption/decryption.
- [x] 24-byte XChaCha nonce generation.
- [x] AEAD AAD for chunk metadata binding.
- [x] X25519 ephemeral key exchange.
- [x] Transcript-bound session key derivation.
- [x] Optional PSK authentication tag support.
- [x] Random reusable file key generation.
- [x] File key wrapping/unwrapping with AEAD.
- [x] `ShareId`.
- [x] `EtleDescriptor`.
- [x] `FileEntry`.
- [x] `CryptoSuite`.
- [x] Deterministic descriptor share ID computation.
- [x] Descriptor serialization/deserialization using `bincode-next`.
- [x] Single-file package layout.
- [x] Directory package layout.
- [x] Logical package stream chunking.
- [x] Unit tests for crypto, descriptor, package, and chunking.

Definition of Done:

- [x] File/folder metadata can be represented as an ETLE share.
- [x] Encrypted chunks are reusable for the same share.
- [x] File key is not part of public descriptor metadata.
- [x] Crypto tests pass.

## Sprint 2 — Protocol and P2P Transfer

Status: complete.

Goal:

- Transfer encrypted chunks through a custom TCP protocol.

Done:

- [x] Length-prefixed `bincode-next` message framing.
- [x] Empty frame rejection.
- [x] Oversized frame rejection.
- [x] Trailing bytes rejection.
- [x] `Hello`.
- [x] `Capabilities`.
- [x] `KeyExchange`.
- [x] `AuthProof`.
- [x] `RequestShare`.
- [x] `RequestManifest`.
- [x] `Manifest`.
- [x] `WrappedFileKey`.
- [x] `Have`.
- [x] `RequestChunk`.
- [x] `Chunk`.
- [x] `Error`.
- [x] Raw chunk frame capability.
- [x] Windowed request capability.
- [x] Seeder TCP listener.
- [x] Client TCP connection.
- [x] Hello handshake.
- [x] X25519 session establishment.
- [x] Optional PSK-authenticated session establishment.
- [x] Manifest transfer.
- [x] Wrapped file key transfer.
- [x] Chunk request/response.
- [x] BLAKE3 encrypted chunk verification.
- [x] AEAD decrypt and final reconstruction.
- [x] Transfer progress logging.

Definition of Done:

- [x] A peer can download a share from a seeder over TCP.
- [x] A wrong/tampered chunk is rejected.
- [x] Transfer supports authenticated and unauthenticated sessions.

## Sprint 3 — Persistent Library and Daemon

Status: complete.

Goal:

- Move from one-shot transfer into a daemon-owned local library.

Done:

- [x] Default library root.
- [x] `.etle/` internal directory.
- [x] Per-share descriptor storage.
- [x] Per-share secret storage.
- [x] Per-share encrypted chunk storage.
- [x] Per-share progress tracking.
- [x] Share listing.
- [x] Local share deletion from the daemon library.
- [x] Seed from local library state.
- [x] Download into local library state.
- [x] Reuse verified chunks when resuming/downloading.
- [x] Daemon foreground mode.
- [x] Multi-share daemon serving.
- [x] IPC socket/pipe path handling.
- [x] IPC commands for seed/list/delete/download/ping/shutdown.
- [x] Event subscription for progress and share changes.
- [x] Broken-pipe tolerant IPC progress handling.
- [x] Startup banner showing local shares and chunk availability.
- [x] Daemon logs for destructive local library operations.

Definition of Done:

- [x] `etled serve` can run as the central local process.
- [x] `etle-cli` can control the daemon through IPC.
- [x] A completed local share is seedable without the original input path.
- [x] A local share can be removed from the daemon library through CLI/GUI IPC.

## Sprint 4 — LAN Discovery

Status: complete.

Goal:

- Find local seeders automatically without always passing `--peer`.

Done:

- [x] UDP discovery protocol.
- [x] Discovery magic/version check.
- [x] Query by `ShareId`.
- [x] Response with peer ID, instance ID, share name, and listen address.
- [x] Broadcast target discovery.
- [x] Multicast target discovery.
- [x] Loopback target discovery for local tests.
- [x] Active interface multicast join helper.
- [x] Deduplication by discovery instance.
- [x] Handling of unspecified listen address `0.0.0.0` by resolving against UDP response source.
- [x] Discovery server verbose diagnostics.
- [x] Discovery integration test for local seeder download.

Definition of Done:

- [x] A downloader can find a LAN seeder for a known `share_id`.
- [x] Windows/Linux LAN discovery works in tested local scenarios.
- [x] Discovery failures can be diagnosed with `-v`.

## Sprint 5 — Multi-peer and Parallel Download

Status: mostly complete.

Goal:

- Download chunks from more than one peer and use parallel workers.

Done:

- [x] CLI accepts repeated `--peer`.
- [x] IPC download command carries peer list.
- [x] Discovery peers can be merged with manual peers.
- [x] Peer availability can be learned through `Have`.
- [x] Request window support.
- [x] Parallel worker count option.
- [x] Chunk-level verification before storing.
- [x] Retry/fallback behavior for failed peers.
- [x] Per-chunk source progress logging.
- [x] Completed chunks can be reused.
- [x] Test coverage for partial seeders, fallback, and parallel download paths.

Still needs hardening:

- [ ] Better dynamic peer hot-add design.
- [ ] Clearer worker metrics in logs/GUI.
- [ ] More backoff and peer scoring.
- [ ] More stress testing under peer churn.

Definition of Done:

- [x] A download can use multiple peers.
- [x] Multiple chunks can be requested concurrently.
- [ ] Swarm behavior is robust under peer churn.

## Sprint 6 — GUI

Status: functional, still improving.

Goal:

- Provide a desktop interface for common ETLE workflows.

Done:

- [x] GTK4/Relm4 GUI binary behind `gui-relm4`.
- [x] Main window.
- [x] Library/share view.
- [x] Seed action through daemon IPC.
- [x] Download action through daemon IPC.
- [x] Delete share action through daemon IPC.
- [x] Inline delete confirmation panel.
- [x] Peer/discovery settings fields.
- [x] PSK field for client/download side.
- [x] Progress/log panel.
- [x] Daemon sync/refresh.
- [x] Responsive layout improvements.
- [x] Windows GTK runtime packaging.
- [x] Windows-only Fluent-like styling and native decoration environment setup.

Still needs polish:

- [ ] Clearer PSK labeling between daemon/server PSK and download/client PSK.
- [ ] Better empty/error states.
- [ ] Better long-running task cancellation UX.
- [ ] More consistent cross-platform icons.
- [ ] GUI integration tests or snapshot/manual checklist.

Definition of Done:

- [x] GUI can operate the daemon for basic seed/download/delete flows.
- [ ] GUI UX is stable enough for non-developer users.
