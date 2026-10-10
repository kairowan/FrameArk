# ADR 0010: Native media-session adapter

- Status: Accepted
- Date: 2026-10-10
- Scope: M1/M2 platform media boundary

## Context

FANP can now negotiate an offer, decode a bounded FAM1 frame, and carry it on a
QUIC media stream. Platform applications still need a single lifecycle-safe
boundary for configuring decoders, routing track packets, and releasing output
surfaces. Reimplementing that state in Kotlin or desktop UI would violate the
Rust ownership boundary and make cleanup inconsistent.

## Decision

`frameark-native::media_session::MediaSession` owns the prepared/streaming/
closed media lifecycle and adapts `frameark-media` packets to the existing
`frameark-api` `VideoRenderer` and `AudioRenderer` traits. It validates the
offer/renderer shape, matches explicit track identifiers, maps renderer errors
to redacted `FrameArkError` values, resets partially configured outputs, and
performs idempotent cleanup on drop.

The adapter does not import Android APIs, decode codecs, or own a `Surface` or
audio device. Android and desktop modules implement the renderer traits and
remain responsible for MediaCodec, AudioTrack, hardware surfaces, thread
affinity, and UI lifecycle.

## Consequences

- The Native control/media path has one shared platform boundary for M1 and M2.
- Platform renderer failures remain diagnosable without leaking native details.
- A real Android implementation can be added without copying FANP state or
  packet parsing into Kotlin.
