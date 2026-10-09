# ADR 0006: M1 Native control profile and backend lifecycle

- Status: Accepted
- Date: 2026-10-09

## Decision

Add `frameark-native` as the FANP control-plane adapter. It serializes bounded
`Offer`, `Start`, `Stop`, and `Status` messages over authenticated control
streams, uses the shared core `Session` state machine, and exposes a small
`SessionBackend` trait for platform-owned decoder/render/audio preparation.

The initial media profile is deliberately narrow: optional H.264 video,
optional Opus/AAC audio, bounded dimensions/frame rate/sample rate/channels,
and a bounded latency target. Receiver policy is explicit and exact; no
fallback is negotiated implicitly. Every request has a sequence ID and every
response echoes it and the resulting core state.

## Lifecycle and cleanup

`Offer` validates the shared capability intersection and receiver policy before
calling `prepare`. A failed or partial prepare marks the backend dirty so
`reset` is attempted. `Start` is legal only after preparation; `Stop`, timeout,
cancellation, malformed input, or receiver drop releases resources and closes
the paired connection. Diagnostics retain only redacted stable event codes.

## Limits

This is an Experimental control profile, not a media pipeline. Encoded video
and audio samples, datagrams, key-frame recovery, clock synchronization,
persistent trust, and platform decoder wiring remain subsequent increments.
