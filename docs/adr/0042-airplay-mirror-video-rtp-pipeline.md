# ADR 0042: Bounded AirPlay mirror video RTP pipeline

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring media boundary

## Decision

Add `MirrorVideoPipeline` to the AirPlay protocol crate. It reuses the bounded
RTP envelope and sequence jitter policy, validates the negotiated payload type,
and assembles marker-delimited H.264 access units from single-NAL, STAP-A, and
FU-A payloads. It preserves the 90 kHz RTP timestamp, negotiated orientation,
and IDR/keyframe signal through the existing `MirrorVideoAccessUnit` contract.

Loss recovery is explicit: `skip_missing_to` advances the sequence window and
discards partial access-unit/FU-A state. The pipeline never fabricates a frame,
conceals loss, decrypts, decodes, opens sockets, or authorizes a sender.

## Bounds and failure policy

RTP jitter capacity, NAL count, payload size, and assembled access-unit bytes
remain bounded by the shared AirPlay constants. Timestamp changes before an
RTP marker, malformed STAP-A lengths, FU-A overlap/end-without-start, wrong
payload types, and unsupported aggregation types are rejected with redacted
typed errors.

## Evidence and limits

Unit tests cover packet reordering, STAP-A, FU-A reconstruction, orientation,
wrong payload type, and recovery after a missing fragment. Apple pairing,
FairPlay/AES-CTR, real UDP sockets, H.264 decoding, reconfiguration, and named
device compatibility remain Experimental and unverified.
