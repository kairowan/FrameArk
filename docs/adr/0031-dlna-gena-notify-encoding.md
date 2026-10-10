# ADR 0031: Bounded DLNA GENA NOTIFY encoding

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP event delivery boundary

## Decision

Add `GenaEvent::encode_http_notify` to turn a validated queued event into one
deterministic HTTP/1.1 `NOTIFY` request. The encoder validates the callback
scheme, authority, path/query target, SID, service type, sequence range, and
event-body size; it emits `HOST`, `NT`, `NTS`, `SID`, `SEQ`, `Content-Type`, and
exact `Content-Length` headers. The bytes are handed to a caller-owned HTTP/TLS
client.

The protocol crate does not resolve hosts, open callback sockets, negotiate
TLS, retry failures, expire leases, or interpret callback responses. This keeps
network policy and authorization outside deterministic GENA state and avoids
turning a subscriber-supplied URL into an implicit fetcher.

## Evidence and limits

Renderer subscription tests generate initial and mutation events, encode a real
`NOTIFY`, and parse it back through the bounded HTTP request parser. Invalid
schemes, authorities, targets, oversized bodies, and out-of-range sequences
remain rejected. Persistent callback I/O and named controller compatibility are
still Experimental.
