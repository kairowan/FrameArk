# ADR 0069: Android negotiated media configuration bridge

- Status: Accepted
- Date: 2026-10-11
- Scope: M1/M2 Android media lifecycle

## Decision

Expose the negotiated FANP video and audio track parameters from Rust to the
Android receiver through a bounded, one-shot `FAMC` envelope. Rust remains the
owner of negotiation and session state. Kotlin only validates and dispatches
the parameters to platform-owned decoder and audio adapters.

`FAMC` version 1 has this fixed layout:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII magic `FAMC` |
| 4 | 1 | Version (`1`) |
| 5 | 1 | Kind (`1` video, `2` audio) |
| 6 | 1 | Codec (`1` H.264, `2` H.265, `3` AAC, `4` Opus, `5` PCM, `6` ALAC, `255` unknown) |
| 7 | 1 | Reserved (`0`) |

Video envelopes append big-endian `u32` width, height, frame-rate numerator,
and denominator (24 bytes total). Audio envelopes append big-endian `u32`
sample rate and channel count (16 bytes total). The Rust bridge stores only the
latest configuration per track; polling consumes it. Starting or stopping the
receiver clears both configurations with the media queue, preventing stale
decoder state from crossing sessions.

## Validation and ownership

The Kotlin bridge accepts only the exact envelope size and expected kind,
requires the reserved byte to be zero, and bounds video dimensions to 7680,
frame-rate fields to 240000, audio sample rates to 8–192 kHz, and channels to
1–8. Unknown codec values are surfaced as `UNKNOWN` rather than guessed.
`MediaPlaybackPump` dispatches configuration before draining encoded `FAMF`
frames. Actual `MediaCodec`, `AudioTrack`, and `Surface` ownership remains a
platform concern and is not implied by this bridge.

## Verification

- `frameark-ffi` loopback integration asserts negotiated H.264 and Opus
  configurations are captured alongside the two media frames.
- Kotlin JVM tests cover valid video/audio envelopes, malformed dimensions, and
  configuration dispatch before the media drain.
- Android CI continues to verify native library packaging; a physical-device
  decoder callback and hardware playback matrix remain open M2 evidence.
