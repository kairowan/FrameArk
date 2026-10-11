# ADR 0036: Bounded AirPlay mirror-audio RTP pipeline

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring audio delivery boundary

## Decision

Add `MirrorAudioPipeline` to compose the existing sequence-aware
`RtpJitterBuffer` with the validated AAC/PCM16 mirror audio access-unit
contract. The pipeline checks the negotiated RTP payload type, derives PCM16
duration from complete interleaved samples, uses the configured packet duration
for AAC, and exposes explicit `skip_missing_to` recovery after a caller-owned
loss deadline.

The pipeline remains a pure protocol/media boundary. It owns no UDP socket,
pairing, FairPlay/AES decryption, decoder, clock feedback, concealment, or
reconfiguration policy.

## Evidence and limits

Tests cover sequence reordering, sample-clock timestamps, AAC packet duration,
PCM alignment, payload-type rejection, bounded capacity, and explicit gap
recovery. Real AirPlay mirror captures, encrypted packets, decoder output,
audio/video synchronization, and Apple device compatibility remain unverified
and Experimental.
