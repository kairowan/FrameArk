# ADR-0085: Gate AirPlay RTSP requests with caller-owned peer authorization

## Status

Accepted — M4/M5 AirPlay transport hardening increment.

## Context

The RAOP and mirror TCP adapters already bounded one request, but an accepted
socket could reach the RTSP state machine before a daemon applied its own
authenticated-peer policy. UDP media had source/SSRC filters, while TCP
control admission remained an implicit allow-all default.

## Decision

Both `RaopRtspTcpServer` and `MirrorRtspTcpServer` expose
`serve_once_with_peer_policy`. The adapter calls the supplied predicate with
the accepted `SocketAddr` before setting timeouts, reading RTSP bytes, or
mutating session state. A `false` result closes the stream and returns an
explicit peer rejection error. The existing `serve_once` API remains a
compatibility helper that deliberately allows every peer for fixture use.

The predicate is caller-owned and may bind the address to a verified RTSP/TLS
identity, pairing record, interface policy, or test fixture. It does not itself
implement Apple pairing, FairPlay, encryption, rate limiting, or identity
verification.

## Verification and limits

Unit coverage checks rejection before request parsing and preservation of the
session state, while existing bounded request/runtime tests continue to cover
accepted traffic. Production deployments must supply an authenticated policy;
the default helper is experimental and must not be exposed directly to an
untrusted network.
