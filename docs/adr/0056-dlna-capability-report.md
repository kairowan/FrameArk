# ADR 0056: DLNA format capability report

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP MediaRenderer capability negotiation

## Decision

Add `MediaFormatCapabilities` to the DLNA MediaRenderer handler. The report
holds a bounded, deterministic list of `ConnectionManager` sink protocol-info
entries, validates control characters and field size, removes duplicates, and
advertises the result through `GetProtocolInfo`. A daemon may replace the
default audio/L16 and video/MP4 subset with an explicitly validated report
matching its actual decoder and renderer capabilities.

This report is declarative: it does not probe remote URLs, fetch media, or
claim decoder support that the platform has not supplied. URL fetching,
authorization, and format probing remain caller-owned.

## Verification

DLNA tests cover deterministic ordering, custom capability output, empty and
control-character rejection, and the existing SOAP/HTTP bounds.
