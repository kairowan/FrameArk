# ADR 0020: Bounded DLNA HTTP and SOAP MediaRenderer slice

- Status: Accepted
- Date: 2026-10-10
- Scope: M3 DLNA/UPnP MediaRenderer control

## Decision

Add a dependency-free, caller-owned HTTP/1.1 parser and
`MediaRendererHttpService` to `frameark-dlna`. The parser accepts one complete
non-chunked request with strict header, body, and `Content-Length` limits. The
handler serves the configured device/SCPD descriptions and maps a bounded
subset of AVTransport, RenderingControl, and ConnectionManager SOAP actions to
one explicit `MediaRendererState`.

Rust owns SOAP action validation and transport/volume state. A daemon or
platform adapter owns TCP accept loops, deadlines, multicast membership,
decoder/rendering, and event delivery. URL selection is limited to HTTP(S);
the protocol crate never performs network fetches.

## Evidence and limits

Unit and handler tests cover HTTP header/body boundaries, XML escaping, normal
SetURI → Play → Seek → Volume → Stop flow, unsupported actions, unsafe URL
schemes, and invalid seek targets. This is still Experimental: GENA eventing,
DIDL-Lite semantics, HTTP Range, network-server integration, and named-client
compatibility are separate increments and must not be inferred from these
tests.
