# ADR 0017: DLNA SSDP and device-description foundation

- Status: Accepted
- Date: 2026-10-10
- Scope: M3 DLNA/UPnP compatibility

## Decision

Create `frameark-dlna` with bounded, dependency-free contracts for complete
SSDP messages and MediaRenderer device descriptions. SSDP parsing normalizes
header names, rejects duplicate/oversized fields, validates body length, and
never exposes an unbounded allocation. Device XML is generated with bounded
service count and XML escaping for all caller-controlled values.

This foundation does not open multicast sockets, serve HTTP/SOAP, implement
AVTransport or RenderingControl actions, subscribe GENA events, or claim DLNA
client interoperability. Each of those layers must add its own limits,
fixtures, and compatibility evidence before the M3 status can move beyond
Experimental.
