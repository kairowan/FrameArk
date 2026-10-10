# ADR 0028: Bounded DLNA TCP MediaRenderer adapter

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP MediaRenderer transport boundary

## Decision

Add `MediaRendererTcpServer` to `frameark-dlna` as a small synchronous adapter
around the existing bounded `HttpRequest` parser and
`MediaRendererHttpService`. It binds a caller-selected address, applies a
non-zero read timeout, accepts one connection per `serve_once` call, waits only
until one complete bounded request is available, writes one bounded response,
and closes the stream. The handler and renderer state remain in Rust; no socket
thread, async runtime, SSDP multicast membership, or platform decoder is
created implicitly.

## Evidence and limits

The loopback test sends a real HTTP GET through a TCP socket and verifies the
HTTP 200 response and device description. Incomplete headers/bodies continue
reading only within `MAX_HTTP_BYTES`; malformed requests and timeouts are
returned as bounded errors. Multi-client scheduling, callback delivery,
streaming file I/O, and named controller compatibility remain future work.
