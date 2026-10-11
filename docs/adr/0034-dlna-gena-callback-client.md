# ADR 0034: Bounded DLNA GENA callback client

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP event delivery boundary

## Decision

Add `GenaCallbackClient` to `frameark-dlna`. It sends the existing bounded
`GenaEvent::encode_http_notify` request to an explicitly port-qualified plain
HTTP callback using a synchronous connect/read/write timeout. The response is
parsed through a bounded, non-chunked HTTP/1.1 response parser and returned as
`GenaCallbackResponse`.

HTTPS URLs are rejected with an explicit error. A daemon or platform adapter
that has reviewed TLS, DNS, and callback authorization policy may wrap the
encoded request in its own vetted client; the protocol crate does not silently
downgrade or invent TLS.

## Evidence and limits

Unit tests cover loopback request/response delivery, exact content length,
header preservation, malformed chunked responses, missing ports, and HTTPS
rejection. The client does not yet implement lease expiry, retry/backoff,
callback authentication, DNS/SSRF policy, persistent connections, or named
DLNA client interoperability.
