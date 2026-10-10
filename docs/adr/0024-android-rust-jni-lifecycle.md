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

The bridge carries no media bytes or secrets and does not own Android codec or
notification objects. Network discovery, pairing, QUIC streams, JNI callbacks,
and MediaCodec/AudioTrack ownership remain separate boundaries.

## Evidence and limits

Rust tests cover session construction, transition validation, and idempotent
state cleanup. JVM tests cover ABI mismatch, native-unavailable behavior, and
start/stop forwarding. Gradle lint/test/assemble and workspace Rust checks pass;
no physical-device JNI loading or network-to-renderer playback is claimed.
