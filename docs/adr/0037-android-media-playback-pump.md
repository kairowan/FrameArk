# ADR 0037: Bounded Android media playback pump

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android receiver platform boundary

## Decision

Add `MediaPlaybackPump` to the Android Receiver shell. It verifies the
FrameArkNative ABI, enables polling only after a successful load, drains at
most a configured bounded number of FAMF envelopes per tick, and routes video
and audio frames to caller-owned consumers. Stop and native-unavailable paths
never poll the queue.

The pump does not decode, configure `MediaCodec`, own a `Surface`, write an
`AudioTrack`, or change Rust protocol/session state. Those responsibilities
remain in platform consumers and the Android service lifecycle.

## Evidence and limits

JVM tests cover ordered routing, empty-queue behavior, unavailable native
libraries, stop behavior, and per-tick drain bounds. Physical JNI loading,
network-driven service attachment, codec output, and hardware playback remain
unverified and Experimental.
