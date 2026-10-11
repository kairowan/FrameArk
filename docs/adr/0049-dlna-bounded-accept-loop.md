# ADR 0049: bounded DLNA TCP accept loop

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP MediaRenderer transport

## Decision

Add `MediaRendererTcpServer::serve_requests` as a synchronous, sequential
accept loop with an explicit request budget capped by
`MAX_TCP_REQUESTS_PER_RUN`. Each connection still receives one bounded HTTP
request and one response, then closes; the existing service state and file
streaming policies are reused without a second parser or unbounded queue.

This is a reference scheduler primitive, not a production concurrent server.
Callers that need cancellation, connection admission, worker isolation, or
TLS must wrap it with a bounded platform-owned scheduler and authorization
policy.

## Verification

The DLNA loopback suite now covers zero/oversized budget rejection and two
sequential device-description requests over separate connections. Existing
range/file, SOAP, GENA, malformed-input, and resource cleanup tests remain the
compatibility evidence.
