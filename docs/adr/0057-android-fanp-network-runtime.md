# ADR 0057: Android FANP network runtime

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver Rust/JNI network boundary

## Decision

Extend `frameark-ffi` with a Rust-owned, ephemeral FANP QUIC listener started
and stopped with the Android receiver lifecycle. The listener generates a
temporary pairing code and self-signed certificate, accepts bounded FANP
pairings, drives `NativeMediaControlReceiver`, and forwards encoded H.264/Opus/
AAC frames into the existing bounded JNI queue. Kotlin may read the ephemeral
port, pairing code, and certificate for an explicit pairing UI; no value is
persisted or logged.

The worker uses a bounded accept timeout and joins during stop, so cancellation
releases the QUIC endpoint and thread. Renderers remain queue adapters: Android
`MediaCodec`/`AudioTrack` and user authorization are still platform-owned, and
the capability remains Experimental until physical-device evidence exists.

## Verification

Rust tests bind and stop the real FANP listener, verify bounded pairing
material, and retain existing JNI queue/lifecycle tests. Android JVM tests cover
the unavailable-before-start network-info state; CI must run the native bridge,
Android lint, and unit tests.
