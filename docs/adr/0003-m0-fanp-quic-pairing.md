# ADR 0003: M0 FANP QUIC/TLS temporary pairing

- Status: Accepted
- Date: 2026-10-09
- Decision owners: FrameArk maintainers

## Context

The mDNS discovery adapter can locate a FrameArk endpoint, but discovery data
is unauthenticated and must not create a media session. The next M0 increment
needs a bounded control handshake that can be exercised by Android, desktop,
and headless receivers without duplicating protocol state in platform code.

## Decision

1. Use QUIC with TLS 1.3 through `quinn` and rustls. FANP advertises the ALPN
   identifier `frameark/1`.
2. Generate an ephemeral self-signed server certificate for each server
   process. The client pins the DER certificate supplied by the in-scope
   discovery/session layer; no system-wide or persistent trust store is
   modified.
3. Use a six-digit decimal pairing code for the first handshake. The code is
   generated from the operating-system-backed RNG, validated as bounded ASCII,
   and discarded after the caller ends the session.
4. Encode control messages in a fixed FANP envelope: `FANP` magic, one-byte
   protocol version, one-byte message type, two-byte big-endian payload length,
   and a payload capped at 256 bytes.
5. Keep protocol framing, pairing decisions, timeouts, and connection state in
   Rust. Platform layers only present the code, supply the pinned certificate,
   and own their OS lifecycle/rendering APIs.

## Consequences

- Android and desktop can share the same pairing behavior and negative-path
  tests.
- The transport is local-network and experimental; it does not provide a
  persistent device identity, media tracks, remote access, or an internet
  relay.
- A later trust milestone must define identity rotation, replay policy, and
  secure platform-backed key storage before pairing can become a remembered
  device relationship.
- Rejection responses are sent before a short bounded connection teardown so a
  client can distinguish an incorrect code from a transport failure.

## Security notes

Certificate pinning is scoped to the certificate bytes explicitly supplied by
the caller. A discovery record alone is not sufficient to authenticate a
peer. The temporary code is not a replacement for a persistent identity and
must not be logged, persisted, or reused as a secret. Protected media, DRM,
HDCP, Cast certification, and universal Miracast support remain outside this
increment.
