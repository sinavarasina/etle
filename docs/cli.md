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

## Output Path

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --output ./received.bin
```

## Fresh Download

A fresh download ignores existing reusable verified chunks for that job. Use this when testing a clean transfer path.

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --fresh
```

If your CLI exposes this as a separate subcommand, check:

```bash
etle-cli --help
```

## PSK Authentication

Downloader PSK must match daemon/seeder PSK:

```bash
etle-cli download \
  --share-id <64_HEX_SHARE_ID> \
  --peer 192.168.1.15:7000 \
  --auth-psk "same-password"
```

## IPC Socket

Use a non-default daemon IPC path:

```bash
etle-cli --ipc-socket /path/to/etled.sock list
```

## Ping and Shutdown

```bash
etle-cli ping
etle-cli shutdown
```

## Help

The CLI help is the source of truth for exact flags in the current build:

```bash
etle-cli --help
etle-cli download --help
etle-cli seed --help
```
