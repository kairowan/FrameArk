# ADR 0011: Android media renderer lifecycle

- Status: Accepted
- Date: 2026-10-10
- Scope: M2 Android Receiver media boundary

## Decision

The Android receiver owns two narrow platform adapters: `VideoDecoderRenderer`
wraps hardware-first H.264 `MediaCodec` output to a caller-owned `Surface`, and
`AudioOutputRenderer` wraps a PCM16 `AudioTrack`. Both adapters have explicit
configure/render/reset methods, reject invalid input, and release native
resources on setup or playback failure.

Rust remains responsible for FANP offers, packet order, timestamps, lifecycle,
and diagnostics. Kotlin does not implement discovery, pairing, capability
negotiation, or a second session state machine. The audio adapter accepts PCM
only; Opus/AAC decoding must be supplied by a separately negotiated backend.

## Limits

This is an Android platform boundary, not the M2 exit condition. It has no
network receiver wiring, no hardware-device matrix, and no claim of 1080p60
stability until real-device tests are added.
