# ADR 0029: Bounded Android JNI media envelope

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver media boundary

## Decision

Extend the existing lifecycle JNI bridge with a process-local FIFO for encoded
video and audio access units. Each submission is limited to 4 MiB and the queue
retains at most 16 frames; a full queue returns an explicit backpressure code.
The poller returns a `FAMF` envelope containing a version, media kind, keyframe
flag, signed big-endian presentation timestamp, payload length, and bytes. Kotlin
validates the envelope, media kind, keyframe rules, length, and ABI state before
exposing a `MediaFrame` to future decoder adapters.

Rust owns queue state and lifecycle cleanup. Kotlin owns JNI loading and will
later connect the decoded envelope to `MediaCodec`/`AudioTrack`; no socket,
network callback, decoder, or Android `Surface` is created by this increment.

## Evidence and limits

Rust unit tests cover deterministic envelope encoding, lifecycle cleanup, queue
capacity, invalid-frame bounds, and backpressure. JVM tests cover queue-full
mapping, envelope decoding, and rejecting empty media before library loading.
Physical-device JNI loading, network-to-queue attachment, and hardware playback
remain unverified and Experimental.
