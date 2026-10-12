# ADR 0104: Make AirPlay mirror capability boundaries explicit

- Status: Accepted
- Date: 2026-10-12
- Milestone: M5

## Context

The experimental AirPlay mirror crate can validate bounded, unencrypted RTP
contracts, but it does not implement Apple pairing, AWDL transport, FairPlay,
or general DRM. A caller that only sees a parser or RTSP state transition could
otherwise report a protected or platform-specific stream as a generic network
failure. That is unsafe for users and makes compatibility evidence ambiguous.

## Decision

Add `MirrorCompatibilityReport` and its input enums to `frameark-airplay`.
Callers provide the observed transport, content-protection state, and pairing
state. The report returns one stable decision and a redaction-safe reason code
and message:

- verified clear-IP input is admitted only to the experimental bounded
  contracts;
- AWDL is reported as conditional on a platform adapter;
- FairPlay and other/unknown protected payloads are rejected explicitly;
- required or unverified pairing is rejected before media handling;
- unknown transports are rejected.

The report is an admission and diagnostics boundary, not an Apple pairing or
decryption implementation. It contains no peer addresses, URLs, device IDs,
or media identifiers.

## Consequences

This prevents silent fallback into unsupported Apple/DRM paths and gives
platform adapters stable diagnostics suitable for UI and support bundles. The
crate remains `Experimental`; decoder/output integration, Apple device
interoperability, production DNS/TLS, AWDL adapters, and protected-content
handling remain separate work and are not implied by this API.

## Evidence

`cargo test -p frameark-airplay --all-features --locked` covers clear-IP,
FairPlay, DRM, AWDL, unknown transport, and pairing-state decisions. The
compatibility matrix and transport notes record the same boundary.
