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
- an explicit file-backed media resource registry that streams bounded HTTP
  Range responses in chunks without loading the file into renderer memory;
- deterministic DIDL-Lite item generation for URI metadata;
- bounded GENA subscription/renew/unsubscribe policy with sequenced event
  bodies, monotonic lease expiry, and stale callback cleanup for caller-owned
  callback HTTP;
- deterministic bounded HTTP `NOTIFY` request encoding for queued GENA events;
- a bounded synchronous plain-HTTP callback client with explicit HTTPS rejection;
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

HTTP chunked transfer, callback connection retries and HTTPS/TLS, full DIDL-Lite parsing,
multicast lease scheduling, network URL fetching, real decoder integration,
and named client interoperability are not implemented yet. File streaming is
opt-in: a daemon must explicitly register a regular file and still provide
authorization, lifecycle, and media format policy. The TCP adapter is
single-request and synchronous, so it is not a production multi-client daemon.
`GenaEvent::encode_http_notify` prepares bytes for a caller-owned HTTP/TLS
client but does not open callback sockets or retry failures.
`GenaCallbackClient` supplies plain-HTTP socket I/O. Subscriptions expire on
monotonic deadlines; subscribe, renew, publish, request handling, and event
draining reap stale records. An optional host timer can call
`expire_subscriptions()` for idle cleanup and record its removal count.
Events already drained to a caller are caller-owned and require a lease check
if delivery is delayed.
The compatibility label is **Experimental**, not Stable.

Run the focused tests with:

```powershell
cargo test -p frameark-dlna --all-features --locked
```
