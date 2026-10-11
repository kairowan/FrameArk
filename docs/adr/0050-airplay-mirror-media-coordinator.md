# ADR 0050: AirPlay mirror media coordinator

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen mirroring media boundary

## Decision

Add `MirrorMediaCoordinator` as the bounded Rust boundary between an
RTSP-negotiated mirror session and the existing RTP video/audio pipelines. It
filters each parsed packet by the negotiated sender IP and per-track SSRC,
routes it through the existing jitter/assembly policy, exposes completed
access-unit counters, supports explicit sequence-gap recovery, and can reset
both pipelines while retaining the negotiated bounds.

The coordinator owns no UDP socket, Apple pairing, encryption, decoder,
surface, or audio output. A platform adapter must obtain packets from
`RtpReceiverSocket`, apply RTSP/session authorization, and consume the returned
access units behind its decoder boundary.

## Verification

The AirPlay suite covers accepted video/audio units, peer and SSRC rejection,
redacted bounded counters, orientation update, and reset cleanup. Existing
RTP reorder/loss, H.264 assembly, audio contract, and mirror RTSP
reconfiguration fixtures remain required.
