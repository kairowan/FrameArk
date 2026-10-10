# ADR 0021: DLNA bounded Range and DIDL-Lite media policy

- Status: Accepted
- Date: 2026-10-10
- Scope: M3 DLNA/UPnP media HTTP responses and metadata

## Decision

Extend the caller-owned `frameark-dlna` HTTP handler with a single-range
`bytes=start-end`/suffix parser, bounded in-memory `MediaResource` fixtures, and
206/416 response headers (`Accept-Ranges`, `Content-Range`, and DLNA
`contentFeatures.dlna.org`). Add a deterministic `DidlLiteItem` generator for
URI metadata. Resource bytes are capped and copied only inside the protocol
helper; a production daemon can replace the source with a streaming backend
without moving session state out of Rust.

## Security and evidence

The parser rejects multiple ranges, unsatisfiable ranges, empty resources,
header injection, and resources over the configured limit before slicing. DIDL
fields are bounded and XML-escaped. Loopback-style handler tests cover full and
partial responses, invalid ranges, suffix ranges, and escaped metadata. This
does not implement file/network fetching, chunked transfer, GENA eventing, or
full DIDL-Lite parsing and is still Experimental.
