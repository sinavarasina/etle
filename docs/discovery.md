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

the discovery response may advertise an unspecified address. The client resolves that using the UDP response source IP.

Example:

```text
response source: 192.168.1.15:37037
listen_addr:     0.0.0.0:7000
resolved peer:   192.168.1.15:7000
```

## Verbose Diagnostics

With `etled -v serve`, discovery server diagnostics should show:

```text
[discovery] server started ...
[discovery] udp packet from ...
[discovery] query from ... share_id=...
[discovery] responding to ...
[discovery] response sent ...
```

Common drops:

```text
drop: decode failed
drop: not a query
drop: bad magic
drop: share not found locally
```

## Troubleshooting

Manual TCP works but discovery fails:

```bash
etle-cli download --share-id <ID> --peer 192.168.1.15:7000
```

If manual peer works, the P2P transfer path is fine and the issue is discovery.

Check:

- daemon is running
- correct library root
- correct share ID
- UDP 37037 reachable
- both devices on same LAN
- router/AP does not block broadcast/multicast
- OS firewall allows UDP 37037
- no stale daemon process
- `etled -v serve` discovery logs

Useful Linux commands:

```bash
ss -Hulpn | grep ':37037'
ss -Hltnp | grep ':7000'
ip -4 addr
ip route
```
