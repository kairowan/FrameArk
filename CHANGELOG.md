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
