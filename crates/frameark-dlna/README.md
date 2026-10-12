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
- a bounded `ConnectionManager` format capability report with deterministic
  custom sink protocol-info entries;
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
- a caller-injected GENA callback connector for HTTP or HTTPS/TLS streams; the
  connector owns DNS, certificate validation, socket timeouts, and retries;
- a bounded media-format probe that extracts MIME type, optional DLNA profile,
  and content length from a caller-owned HTTP response and matches it against
  the renderer capability report;
- a caller-injected bounded remote media range fetch that validates DNS/network
  policy, requires a matching `206 Content-Range` for requested chunks, rejects
  redirects and range-ignoring origins, and returns at most one bounded body;
- a caller-injected bounded remote media range stream that schedules exact
  inclusive chunks up to a 512 MiB resource/4096-request budget without
  buffering the complete object;
- an explicit-root Rustls HTTPS connector for media URIs with bounded caller
  DNS, address-policy validation, SNI verification, socket timeouts, and no
  redirect or system-trust-store behavior;
- a bounded payload-free DLNA replay trace for SSDP, HTTP, SOAP/GENA, and
  media-range counters without retaining XML, headers, URLs, credentials, or
  media bytes;
- explicit transport, position, URI, metadata, and volume state with cleanup
  left to the caller-owned connection/session lifecycle.
- a caller-injected, bounded DNS-aware media URI policy for public-only or
  local-network-only fetchers; the protocol crate never performs DNS or URL
  fetching itself;

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
`http://` and `https://`; the bounded URI validator rejects credentials,
control/whitespace characters, fragments, malformed hosts, invalid ports, and
unbracketed IPv6 authorities. FrameArk does not fetch URLs or bypass DRM.

HTTP chunked transfer, callback connection retries, full
DIDL-Lite parsing,
multicast lease scheduling, built-in network URL fetching, real decoder integration,
and named client interoperability are not implemented yet. File streaming is
opt-in: a daemon must explicitly register a regular file and still provide
authorization, lifecycle, and media format policy. The TCP adapter is
single-request and synchronous, so it is not a production multi-client daemon.
`validate_media_uri_network` requires callers to resolve a hostname with a
bounded resolver and checks every returned address, rejecting empty/oversized
sets, multicast/unspecified addresses, private addresses for
`PublicNetworkOnly`, and public addresses for `LocalNetworkOnly`. It is a
preflight policy boundary rather than a fetcher and cannot by itself prevent a
later redirect or DNS change. `connect_https_media_uri_with_roots` supplies an
explicit-root Rustls connector: callers provide bounded DNS results and trust
anchors, while the connector rejects non-HTTPS URIs, verifies SNI, installs
socket timeouts, and never follows redirects or loads system roots. It does
not replace caller-owned authentication, cancellation, or redirect policy.
`GenaEvent::encode_http_notify` prepares bytes for a caller-owned HTTP/TLS
client but does not open callback sockets or retry failures.
`send_gena_callback_with_connector` reuses the bounded request and response
parser with a vetted TLS stream, but does not implement TLS or prove peer
authentication. `MediaFormatProbe` is metadata-only. The
`fetch_media_range_with_connector` adds one bounded range request and
`stream_media_ranges_with_connector` schedules a known-length sequence of
bounded ranges. Neither follows redirects, inspects a decoder, or overrides
authorization and DNS policy. The
caller owns cancellation, connector security, buffering, and output; unknown
resource lengths are rejected rather than fetched until exhaustion.
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
