# ADR 0072: AirPlay mirror codec boundary

> Superseded by ADR-0076 for the H.265 assembly increment. The H.264
> compatibility and explicit codec-enum decisions remain applicable.

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring SETUP

## Decision

Parse the optional `codec` value in a mirror SETUP property list. Missing
codec fields remain compatible with the existing H.264 contract. Explicit
`h264`, `h.264`, or `avc` values select the current bounded H.264 RTP
assembler. Explicit `h265`, `h.265`, or `hevc` values fail with an explicit
unsupported-codec error; unknown or non-string values fail as malformed
configuration.

The public configuration model carries a `MirrorVideoCodec` enum so callers
cannot mistake an omitted field for a future H.265 implementation. At the time
of this decision no H.265 NAL assembly, decoder, Apple pairing, FairPlay, or
hardware compatibility was claimed.

## Verification

- Existing XML and binary plist H.264 SETUP/lifecycle tests remain green.
- New coverage rejects explicit H.265 and unknown codec values before transport
  or session state is mutated.
- AirPlay unit tests, clippy, and the compatibility matrix record the
  conditional boundary.
