# ADR 0089: DLNA caller-owned format probe connector

- Status: Accepted
- Scope: M3 DLNA media push

## Context

DLNA `SetAVTransportURI` accepts an HTTP(S) resource, but the protocol crate
must not become an implicit SSRF client or download an unbounded media object.
The existing format probe validated a response only and left the request
boundary to every daemon implementation.

## Decision

Add `fetch_media_format_with_connector`. It applies the caller-supplied DNS
and network-class policy, emits one bounded `GET` with `Range: bytes=0-0`,
parses one bounded HTTP response, rejects redirects through the 2xx-only
format probe, and returns the existing `MediaFormatProbe`. DNS, TLS,
authentication, cancellation, redirect handling, and streaming remain
caller-owned; no media bytes are downloaded into renderer state.

## Consequences

- Daemons can reuse one tested request/response boundary before selecting a
  decoder or beginning playback.
- An origin that ignores Range may be rejected by the bounded HTTP parser
  rather than causing an unbounded transfer.
- Full URL fetching, decoder inspection, and named controller compatibility
  remain open M3 work.
