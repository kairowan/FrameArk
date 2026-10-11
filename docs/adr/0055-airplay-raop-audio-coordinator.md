# ADR 0055: RAOP audio packet coordinator

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay/RAOP audio media boundary

## Decision

Add `RaopAudioCoordinator` as the bounded Rust boundary between a negotiated
RAOP session and an audio decoder/output adapter. It filters caller-supplied
RTP packets by the authenticated peer IP, negotiated SSRC, and payload type,
then applies the existing sequence-aware jitter buffer. It exposes ordered
packets, explicit missing-sequence recovery, reset, and redacted counters.

The coordinator owns no UDP socket, Apple pairing, FairPlay/AES-CTR decryption,
codec decoder, audio device, or authorization policy. A caller must derive the
peer/SSRC from its RTSP boundary and decide when transport loss is safe to
skip.

## Verification

The AirPlay suite covers accepted and ordered packets, wrong-peer and wrong-SSRC
rejection, explicit gap recovery, bounded counters, and reset cleanup.
