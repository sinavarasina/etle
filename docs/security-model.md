# Security Model

ETLE is an experimental encrypted P2P transfer project. This document explains what the current design tries to protect and what it does not protect.

## Goals

ETLE aims to provide:

- encrypted chunk transfer
- encrypted local chunk storage
- chunk corruption detection
- descriptor/share identity checks
- optional PSK-based MITM resistance
- protection against accidental wrong-share/wrong-chunk use

## Non-goals

ETLE does not currently provide:

- anonymity
- traffic hiding
- public identity certificates
- DHT trust
- NAT traversal security
- malicious peer reputation
- formal audit guarantees
- production hardening against DoS

## Assets

Sensitive:

- `secret.etlekey`
- file key
- PSK passphrase
- plaintext reconstructed output

Public/shareable:

- `descriptor.etle`
- share ID
- chunk count/size metadata
- encrypted chunks, assuming file key remains secret

Network-visible:

- peer IP/port
- share ID being queried
- transfer timing/size
- discovery packets

## Threats

### Passive Network Observer

Should not learn plaintext file content because chunks are encrypted.

May learn:

- IP addresses
- ports
- timing
- share ID
- approximate transfer size

### Active MITM

Without PSK:

- X25519 alone does not authenticate peer identity.
- MITM may establish separate sessions.

With PSK:

- MITM without PSK should fail authentication.
- PSK must be strong enough and shared out-of-band.

### Malicious Peer

A malicious peer may:

- send corrupt chunks
- lie in Have list
- disconnect repeatedly
- send wrong descriptor/manifest
- slow down transfer

Mitigations:

- BLAKE3 chunk verification
- AEAD authentication
- share ID validation
- retry/fallback
- frame size limits

Still needed:

- stronger peer scoring
- rate limits
- abuse throttling

### Local Attacker

If local attacker can read `secret.etlekey`, they can decrypt the share.

Mitigations not yet implemented:

- encrypted local secret store
- OS keychain integration
- passphrase-protected library

## Security Checklist for Changes

When changing protocol/crypto/state code, check:

- Does this expose file key or PSK?
- Does this accept unauthenticated metadata too early?
- Is every network frame size-limited?
- Is every chunk verified before progress is updated?
- Is AAD stable and unambiguous?
- Does this change require a protocol version bump?
- Does this change break backward compatibility?
