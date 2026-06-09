# Daemon and IPC

`etled` is the local daemon. It owns network listeners and the local library.

## Daemon Responsibilities

- Load configuration.
- Resolve library root.
- Print local share inventory.
- Listen for TCP P2P connections.
- Serve local shares.
- Start UDP discovery server.
- Start IPC command server.
- Apply seed/download/delete commands.
- Emit progress/events to clients.
- Apply PSK and log-level settings.

## Start Daemon

```bash
etled -v serve
```

Common options:

```bash
etled -v serve \
  --listen 0.0.0.0:7000 \
  --discovery-port 37037
```

Disable discovery:

```bash
etled -v serve --no-discovery
```

Disable IPC:

```bash
etled -v serve --no-ipc
```

Use a different library root:

```bash
etled -v serve --library-root ./ETLE-LIBRARY
```

Use PSK authentication:

```bash
etled -v serve --auth-psk "same-password"
```

or:

```bash
ETLE_AUTH_PSK="same-password" etled -v serve
```

## IPC

The CLI and GUI communicate with `etled` through IPC.

Typical IPC commands:

- Ping
- ListShares
- SeedFile
- DeleteShare
- Download
- DownloadFresh
- SubscribeEvents
- Shutdown

Typical IPC responses/events:

- share list
- share added
- share deleted
- transfer queued
- transfer progress
- transfer completion
- server started/stopped
- errors

Delete requests are handled by the daemon so that CLI and GUI follow the same local-library path. The daemon logs delete request, success, not-found, and error cases.

## IPC Path

Default IPC path is derived from the library root. Unix-like platforms use a socket under the ETLE root:

```text
<library-root>/.etle/etled.sock
```

Windows uses a named pipe:

```text
\\.\pipe\etled
```

Use `--ipc-socket` in CLI when targeting a non-default daemon/library. On Windows, prefer the default named pipe unless you intentionally changed the daemon endpoint.

## Operational Notes

Only one daemon should own a library root at a time. If discovery or IPC seems stale, check for old `etled` processes.

CLI, GUI, and daemon should be rebuilt and updated together whenever IPC message variants change.
