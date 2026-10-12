# ADR 0111: Payload-free AirPlay replay traces

- Status: Accepted
- Date: 2026-10-12
- Milestone: M4/M5 diagnostics and compatibility evidence

## Decision

`frameark-airplay` provides `AirplayReplayTrace` and typed event constructors
for RTSP, RAOP audio RTP, mirror video RTP, and compatibility decisions. A
trace records only a bounded sequence, stable event code, direction, and
protocol counters. It renders deterministic tab-separated text suitable for a
support bundle or state-machine regression reference.

## Security consequences

The trace intentionally cannot replay media or reconstruct a session: packet
payloads, URLs, addresses, credentials, pairing material, keys, and persistent
device identifiers are never stored. Callers must still sanitize any external
diagnostic fields before passing them to other exporters. Protected/FairPlay
content remains outside the decoder and replay boundary.

## Evidence

Unit tests cover capacity bounds, oldest-event eviction, deterministic output,
RTSP/RTP counter retention, and compatibility decision redaction. The feature
does not substitute for named Apple-device captures or long-run interoperability
records; those remain required by the M4/M5 exit gates.
