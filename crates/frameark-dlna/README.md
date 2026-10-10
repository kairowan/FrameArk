# frameark-dlna

`frameark-dlna` contains the bounded Rust protocol layer for FrameArk's
DLNA/UPnP MediaRenderer work.

## Current capability

- bounded SSDP parsing/encoding and a caller-owned UDP publisher;
- escaped UPnP root-device and service descriptions;
- bounded HTTP/1.1 request parsing and deterministic responses;
- an in-process MediaRenderer handler for `AVTransport`,
  `RenderingControl`, and `ConnectionManager` SOAP actions;
- single-range HTTP media responses with bounded in-memory resources and
  DLNA response headers;
- deterministic DIDL-Lite item generation for URI metadata;
- explicit transport, position, URI, metadata, and volume state with cleanup
  left to the caller-owned connection/session lifecycle.

The handler is intentionally not a TCP server. A daemon or platform service
owns sockets, request timeouts, multicast membership, and media rendering, then
passes one complete request to `MediaRendererHttpService`. This keeps protocol
state in Rust and makes malformed-request tests deterministic.

## Supported subset

The current experimental subset serves a device description and SCPD documents
and handles `SetAVTransportURI`, `Play`, `Pause`, `Stop`, `Seek`,
`GetTransportInfo`, `GetPositionInfo`, `GetMediaInfo`, `SetVolume`,
`GetVolume`, and the basic ConnectionManager queries. Media URLs are limited to
`http://` and `https://`; FrameArk does not fetch URLs or bypass DRM.

HTTP chunked transfer, GENA event subscriptions, full DIDL-Lite parsing,
multicast lease scheduling, streaming file backends, real decoder integration,
and named client interoperability are not implemented yet. The compatibility
label is **Experimental**, not Stable.

Run the focused tests with:

```powershell
cargo test -p frameark-dlna --all-features --locked
```
