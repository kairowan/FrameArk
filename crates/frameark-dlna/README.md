# frameark-dlna

`frameark-dlna` contains the bounded Rust protocol layer for FrameArk's
DLNA/UPnP MediaRenderer work.

## Current capability

- bounded SSDP parsing/encoding and a caller-owned UDP publisher;
- escaped UPnP root-device and service descriptions;
- bounded HTTP/1.1 request parsing and deterministic responses;
- a bounded synchronous TCP adapter with read timeouts and one-request
  connection cleanup;
- an in-process MediaRenderer handler for `AVTransport`,
  `RenderingControl`, and `ConnectionManager` SOAP actions;
- single-range HTTP media responses with bounded in-memory resources and
  DLNA response headers;
- deterministic DIDL-Lite item generation for URI metadata;
- bounded GENA subscription/renew/unsubscribe policy with sequenced event
  bodies for caller-owned callback HTTP;
- explicit transport, position, URI, metadata, and volume state with cleanup
  left to the caller-owned connection/session lifecycle.

`MediaRendererTcpServer` is a deliberately small socket boundary: it serves
one complete request per call, closes the connection after the response, and
leaves accept-loop scheduling, multicast membership, and media rendering to a
daemon or platform service. The handler remains reusable for asynchronous
servers and malformed-request tests.

## Supported subset

The current experimental subset serves a device description and SCPD documents
and handles `SetAVTransportURI`, `Play`, `Pause`, `Stop`, `Seek`,
`GetTransportInfo`, `GetPositionInfo`, `GetMediaInfo`, `SetVolume`,
`GetVolume`, and the basic ConnectionManager queries. Media URLs are limited to
`http://` and `https://`; FrameArk does not fetch URLs or bypass DRM.

HTTP chunked transfer, GENA lease expiry scheduling, callback connection I/O,
full DIDL-Lite parsing,
multicast lease scheduling, streaming file backends, real decoder integration,
and named client interoperability are not implemented yet. The TCP adapter is
single-request and synchronous, so it is not a production multi-client daemon.
The compatibility label is **Experimental**, not Stable.

Run the focused tests with:

```powershell
cargo test -p frameark-dlna --all-features --locked
```
