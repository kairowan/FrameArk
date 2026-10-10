# ADR 0024: Android Rust/JNI receiver lifecycle bridge

- Status: Accepted
- Date: 2026-10-10
- Scope: M2 Android Receiver and FFI boundary

## Decision

Extend `frameark-ffi` with versioned JNI entry points for
`nativeStartReceiver`, `nativeStopReceiver`, and `nativeReceiverRunning`.
Rust stores one process-local `frameark-core::Session` behind a mutex and
performs the bounded Idle → Connecting → Closed lifecycle. Kotlin's
`FrameArkNative` validates the ABI, maps integer results to typed outcomes, and
the foreground service forwards explicit start/stop actions.

The bridge carries no secrets and does not own Android codec or notification
objects. A follow-on bounded `FAMF` queue accepts encoded video/audio bytes,
applies explicit backpressure, and returns frames through a JNI byte-array
poller; it remains a data boundary, not a decoder or renderer. Network
discovery, pairing, QUIC attachment, and MediaCodec/AudioTrack ownership remain
separate boundaries.

## Evidence and limits

Rust tests cover session construction, transition validation, idempotent state
cleanup, media queue bounds, backpressure, and deterministic envelope bytes.
JVM tests cover ABI mismatch, native-unavailable behavior, start/stop
forwarding, media backpressure mapping, envelope decoding, and pre-load input
validation. Gradle lint/test/assemble and workspace Rust checks pass; no
physical-device JNI loading or network-to-renderer playback is claimed.
