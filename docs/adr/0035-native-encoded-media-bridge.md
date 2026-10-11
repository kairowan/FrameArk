# ADR 0035: Native encoded-media bridge to the Android JNI queue

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Native media delivery and Android platform boundary

## Decision

Add `EncodedMediaSink` to the platform API and
`NativeEncodedMediaReceiver` to `frameark-native`. The receiver consumes the
already-negotiated, bounded FAM1 stream, validates and decodes its wire
envelope, and forwards encoded video/audio access units to a caller-owned sink.
`frameark-ffi` supplies `JniMediaQueueSink`, which adapts those samples to the
existing process-local, bounded JNI polling queue.

Rust continues to own FANP framing, timestamps, track identity, and transport
cleanup. Android remains responsible for polling, codec configuration,
`MediaCodec`/`AudioTrack`, surfaces, and lifecycle/UI integration.

## Evidence and limits

Loopback QUIC tests prove video/audio FAM1 samples reach a generic encoded sink,
and the FFI unit test proves the JNI sink preserves kind, timestamp, keyframe,
and payload bounds. A real Android native-library load, service-owned network
session, codec callback, hardware decoder matrix, and long-run playback remain
unverified and Experimental.
