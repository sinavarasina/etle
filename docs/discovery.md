# LAN Discovery

ETLE discovery lets a downloader find a local seeder for a known `share_id` without manually entering `--peer`.

## Defaults

```text
UDP port: 37037
Multicast: 239.255.0.86
```

The daemon listens on UDP discovery and answers queries for locally seedable shares.

## Query

A downloader sends a discovery query containing:

```text
magic
share_id
```

The discovery client sends to broadcast, multicast, and loopback targets according to configured options.

## Response

A seeder responds with:

```text
magic
share_id
listen_addr
listen_port
peer_id
instance_id
name
```

`instance_id` is used to deduplicate responses from the same daemon/share source.

## Unspecified Listen Address

If the daemon listens on:

```text
0.0.0.0:7000
```

then discovery may return an unspecified listen address. The client resolves it using the UDP response source address so the peer can connect to the actual interface address.

## Verbose Diagnostics

Run the daemon with:

```bash
etled -v serve
```

Useful messages include:

```text
[discovery] query from ...
[discovery] drop: share not found locally ...
[discovery] responding to ...
[discovery] response sent ...
```

## Troubleshooting

If explicit `--peer` works but discovery fails, the transfer layer is probably fine. Check:

- discovery UDP port
- firewall rules
- multicast/broadcast support on the LAN
- wrong `share_id`
- stale daemon with old library root
- local share was deleted or no longer has descriptor/secret/chunks
