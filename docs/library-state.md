# Library State

The daemon owns a local ETLE library root. By default it is:

```text
Linux/macOS: ~/Downloads/ETLE
Windows:     %USERPROFILE%\Downloads\ETLE
```

Inside that root, ETLE stores internal data in `.etle/`.

## Conceptual Layout

```text
ETLE/
├── .etle/
│   ├── etled.sock              # Unix IPC socket on Unix-like systems
│   └── library/
│       └── <share_id>/
│           ├── descriptor.etle
│           ├── secret.etlekey
│           ├── state.bin
│           ├── progress.bin
│           ├── chunks/
│           │   ├── 000000.etle
│           │   ├── 000001.etle
│           │   └── ...
│           └── output/
└── user-visible files may also live here
```

On Windows the default IPC endpoint is a named pipe, so the socket file shown above is Unix-only. The local library ownership model is otherwise the same.

## Share Modes

A share can conceptually be:

- `Seeding`
- `Downloading`
- `Completed`
- `Paused`

A completed share can be seedable if the descriptor, secret, and required chunks exist.

## Important Files

### `descriptor.etle`

Public metadata. It identifies the share and the chunk/file layout.

Must not contain:

- plaintext file key
- PSK
- local-only credentials

### `secret.etlekey`

Local secret metadata. It contains the reusable file key or enough local secret state to decrypt chunks.

Must be protected.

### `chunks/`

Verified encrypted chunks. Chunks are stored encrypted so they can be reused for seeding.

### `progress.bin`

Tracks which chunks have been verified and stored.

### `state.bin`

Local operational state such as mode/output information.

## Resume

Resume works by:

1. Loading descriptor.
2. Loading progress.
3. Scanning verified chunks.
4. Requesting only missing chunks.
5. Storing newly verified chunks.
6. Reconstructing once complete.

## Seed After Download

After all chunks are verified and the secret key exists, the local peer can serve those chunks to other peers.

## Delete Share

A share can be removed from the local daemon library through IPC:

```bash
etle-cli delete --share-id <64_HEX_SHARE_ID>
```

The daemon removes the local per-share directory under `.etle/library/<share_id>/`. This deletes local descriptor, secret, state, progress, and encrypted chunks for that share.

Deletion does not:

- remove copies from other peers
- invalidate the share ID globally
- securely wipe disk blocks
- delete arbitrary paths outside the share directory
