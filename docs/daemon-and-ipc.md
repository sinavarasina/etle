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
- Seed
- Download
- DownloadFresh
- Shutdown

Typical IPC events:

- log/status messages
- transfer progress
- download completion
- errors

## IPC Path

Default IPC path is derived from the library root. On Unix-like systems this is a socket under the ETLE root. On Windows it is implemented using the platform IPC mechanism used by the project.

Use `--ipc-socket` in CLI when targeting a non-default daemon/library.

## Operational Notes

Only one daemon should own a library root at a time. If discovery or IPC seems stale, check for old `etled` processes.
