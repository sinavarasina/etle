# CLI Usage

`etle-cli` controls `etled` through IPC.

Always start the daemon first:

```bash
etled -v serve
```

## Version

```bash
etle-cli --version
etle-cli version
```

## Seed

Add a file into the daemon library and make it seedable:

```bash
etle-cli seed ./sample.bin
```

With explicit chunk size:

```bash
etle-cli seed ./sample.bin --chunk-size 1048576
```

## List

List local shares known by the daemon:

```bash
etle-cli list
```

## Delete

Delete one share from the daemon-owned local library:

```bash
etle-cli delete --share-id <64_HEX_SHARE_ID>
```

This removes ETLE metadata, secret state, progress, and encrypted chunks for that local share. It does not delete remote copies and is not intended to remove already reconstructed output files outside the share state.

## Download by Discovery

If no `--peer` is supplied, the daemon/client flow can try LAN discovery for the requested share:

```bash
etle-cli download --share-id <64_HEX_SHARE_ID>
```

## Download by Manual Peer

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000
```

## Multiple Peers

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --peer 192.168.1.20:7000 \
  --parallel 4
```

`--parallel 0` means automatic worker count based on resolved peers. `--parallel 1` gives sequential fallback behavior.

## Output Path

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --output ./received.bin
```

If no output is supplied, the daemon uses the library output location based on the manifest file name.

## Fresh Download

A fresh download ignores existing reusable verified chunks for that job. Use this when testing a clean transfer path.

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --no-resume
```

The regular `download` command reuses verified chunks by default.

## PSK Authentication

Downloader PSK must match daemon/seeder PSK:

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --auth-psk "same-password"
```

If `--auth-psk` is omitted, the CLI can use `ETLE_AUTH_PSK` or the configured `auth_psk` value.

## IPC Socket

Use a non-default daemon IPC path:

```bash
etle-cli --ipc-socket /path/to/etled.sock list
```

On Windows the default IPC endpoint is the named pipe:

```text
\\.\pipe\etled
```

## Ping and Shutdown

```bash
etle-cli daemon ping
etle-cli daemon shutdown
```

Watch daemon events and transfer progress:

```bash
etle-cli daemon watch
```

The top-level `list` command and `daemon list` both query daemon shares.

## Help

The CLI help is the source of truth for exact flags in the current build:

```bash
etle-cli --help
etle-cli download --help
etle-cli seed --help
etle-cli daemon --help
```
