# ADR 0026: Bounded AirPlay mirror video contracts

- Status: Accepted
- Date: 2026-10-10
- Scope: M5 AirPlay screen-mirroring foundation

## Decision

Add a protocol-only mirror video contract to `frameark-airplay`. It validates
bounded H.264 Annex-B and four-byte AVCC access units, rejects malformed NAL
headers and empty units, identifies IDR keyframes, carries explicit 90 kHz
presentation timestamps and orientation, and provides a bounded audio-sample
to-video-clock conversion policy.

The contract does not open sockets, decode H.264, render a surface, infer
orientation from an Apple packet, or synchronize a platform audio device. Those
remain platform/session responsibilities and must consume the explicit state
without silently accepting unsupported codecs or dimensions.

## Evidence and limits

Tests cover Annex-B/AVCC parsing, keyframe detection, deterministic re-encoding,
orientation validation, malformed length/header rejection, sample-clock
conversion, and A/V offset bounds. H.265, mirror audio transport, encryption,
loss recovery, real Apple packet captures, and device compatibility remain
unimplemented.
