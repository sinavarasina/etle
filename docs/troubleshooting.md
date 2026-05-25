# Troubleshooting

## Manual Peer Works, Discovery Fails

If this works:

```bash
etle-cli download --share-id <ID> --peer 192.168.1.15:7000
```

but discovery does not, the transfer layer is fine. Check UDP discovery.

On Linux seeder:

```bash
ss -Hulpn | grep ':37037'
ss -Hltnp | grep ':7000'
```

Run daemon verbose:

```bash
etled -v serve
```

Look for:

```text
[discovery] udp packet from ...
[discovery] query from ...
[discovery] response sent ...
```

If there is no UDP packet:

- firewall/router/AP may block broadcast/multicast
- wrong LAN/subnet
- wrong discovery port
- wrong machine/IP

If there is `share not found locally`:

- wrong `share_id`
- wrong library root
- share has no descriptor/secret/chunks
- stale daemon

## Windows Finds Itself

This can happen when the Windows machine also has the same share in its local library.

Test with:

- empty Windows library root
- explicit `--peer` to Linux
- daemon verbose logs on Linux

## `Cannot assign requested address`

Do not pass an IP address into `--discovery-port`. It expects only a number.

Correct:

```bash
etled -v serve --listen 192.168.1.15:7000 --discovery-port 37037
```

Incorrect:

```bash
etled -v serve --discovery-port 192.168.1.15:37037
```

## Multiple Interfaces on One Subnet

If Linux has Ethernet and WiFi both on the same subnet, routing can be confusing.

Check:

```bash
ip -4 addr
ip route
ip route get <peer-ip>
```

For debugging, temporarily disable one interface or use an explicit manual peer.

## PSK Mismatch

If one side uses PSK and the other side does not, the session should fail.

Seeder:

```bash
etled -v serve --auth-psk "same-password"
```

Downloader:

```bash
etle-cli download --share-id <ID> --peer <IP:PORT> --auth-psk "same-password"
```

## GUI Does Not Apply Server PSK

The GUI download PSK is client-side. Server-side PSK must be configured when `etled serve` starts.

## GTK Build Fails

Linux:

```bash
pkg-config --modversion gtk4
```

If that fails, install GTK4 development packages.

Windows CI uses MSYS2 UCRT64. Local Windows builds should use the same environment when possible.

## Stale Daemon

Check for old daemon processes:

```bash
pgrep -a etled
```

Stop old daemon before testing a new binary:

```bash
pkill etled
```
