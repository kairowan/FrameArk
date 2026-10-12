# ADR 0112: Payload-free DLNA replay traces

- Status: Accepted
- Date: 2026-10-12
- Milestone: M3 diagnostics and compatibility evidence

## Decision

`frameark-dlna` provides `DlnaReplayTrace` and typed event constructors for
SSDP, HTTP, SOAP/GENA, and bounded media-range observations. A trace retains
only a bounded sequence, stable code, direction, and protocol counters, then
renders deterministic tab-separated text for support bundles and regression
references.

## Security consequences

The trace is intentionally not a packet capture. XML, headers, URLs,
addresses, credentials, and media bytes are never retained. Callers must keep
redirect, DNS, authentication, and media authorization policy outside the
trace and sanitize any external fields before exporting them.

## Evidence

Unit tests cover capacity bounds, oldest-event eviction, deterministic output,
SSDP/HTTP/SOAP/media counter retention, and absence of untrusted text. This
does not substitute for named Android/iOS/desktop controller runs or the
two-hour playback/Seek exit gate.
