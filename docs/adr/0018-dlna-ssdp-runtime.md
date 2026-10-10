# ADR 0018: DLNA SSDP runtime publisher

- Status: Accepted
- Date: 2026-10-10
- Scope: M3 DLNA/UPnP discovery runtime

## Decision

Build `SsdpPublisher` on a caller-owned bounded `UdpSocket`. It emits one
validated `ssdp:alive`/`ssdp:byebye` notification and responds to matching
`M-SEARCH` requests for `ssdp:all`, the configured service type, or the
configured USN. The socket bind address and target are explicit so local
loopback tests and platform multicast policies remain separate from protocol
state. Multicast group joins, repeated lease scheduling, and interface
selection stay in the platform/service layer.

## Verification and limits

Real UDP loopback tests parse a notification and a matching search response,
and verify unrelated search targets are ignored. This is not yet a DLNA
MediaRenderer: HTTP device serving, SOAP AVTransport/RenderingControl,
GENA events, HTTP Range, and named-client compatibility remain outstanding.
