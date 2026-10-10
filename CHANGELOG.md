# Changelog

All notable FrameArk changes will be documented in this file. The project follows semantic versioning once public artifacts are released.

## Unreleased

### Added

- Experimental `frameark-media` crate with bounded encoded video/audio packets
  and explicit FIFO backpressure semantics.
- Experimental `FAM1` media-frame envelope with bounded exact-boundary
  serialization for native video and audio access units.
- Experimental bounded QUIC media stream with explicit per-frame deadlines and
  sender-owned unidirectional framing.
- Experimental Native media-session adapter connecting validated packets to
  platform-owned video/audio renderer interfaces with cleanup guarantees.
- Experimental Native media receiver orchestration decoding bounded FAM1
  records from QUIC, routing them to renderers, reporting counters, and
  cleaning up on stream termination or failure.
- Experimental `NativeMediaControlReceiver` completing one bounded Offer →
  Start → media → Stop sequence with backend and renderer cleanup guarantees.
- Experimental `frameark-cli send` fixture sender for pinned FANP pairing and
  bounded FAM1 media access units supplied by the caller, now using the Native
  control lifecycle before and after the stream.
- Experimental Android `MediaCodec` H.264 and PCM `AudioTrack` renderer
  lifecycles with explicit setup, backpressure, and release paths.
- Bounded authenticated control request/response streams with cancellation
  cleanup, transport-acknowledged responses, and deterministic lifecycle tests.
- Experimental `frameark-native` control adapter with bounded Offer/Start/Stop/
  Status messages, explicit receiver policy, and backend cleanup hooks.

- Experimental `frameark-transport` crate with bounded FANP v1 control frames,
  QUIC/TLS certificate pinning, process-local temporary pairing, and bounded
  capability negotiation mapped to the shared Session state machine.
- Experimental `frameark-discovery` crate with bounded mDNS/DNS-SD publication
  and normalized browse events for `_frameark._udp.local.`.
- Product and engineering plan.
- FrameArk development Skill and branch guard.
- Open-source governance, collaboration templates, and automated repository checks.
- Rust workspace with `frameark-core` session contracts and `frameark-api` platform interfaces.
- Versioned native ABI constant for platform bindings.
- Minimal Android Receiver shell with a version-checked Rust JNI bridge.
- ADR 0001 freezing the M0 package, discovery, protocol, and ownership boundaries.

### Security

- Pull-request dependency review and CodeQL workflow analysis.
- Documented private vulnerability-reporting process and protected-branch policy.
