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
- Experimental Android Receiver foreground-service lifecycle with explicit
  start/stop actions, playback notification channel, and M2 manifest baseline.
- Experimental Android Keystore-backed P-256 device identity boundary with
  redaction-safe public-key fingerprints; real-device provisioning pending.
- Experimental Android Rust/JNI receiver lifecycle bridge with protected core
  session start/stop/running calls and typed Kotlin result mapping; physical
  device loading and network/media callbacks remain pending.
- Experimental bounded Android Rust/JNI encoded media queue with explicit
  video/audio backpressure, versioned `FAMF` polling envelopes, and Kotlin
  media-kind/PTS/keyframe decoding; physical-device loading, network attachment,
  and hardware renderer callbacks remain pending.
- Experimental `frameark-dlna` foundation with bounded SSDP parsing and
  escaped UPnP device-description generation; network/SOAP interoperability
  remains unimplemented.
- Experimental bounded DLNA SSDP UDP publisher with alive/byebye and matching
  M-SEARCH response loopback coverage; multicast scheduling remains outside
  the crate.
- Experimental bounded DLNA HTTP/SOAP MediaRenderer handler serving device and
  SCPD descriptions and applying AVTransport, RenderingControl, and
  ConnectionManager actions to explicit Rust state; GENA, DIDL-Lite, HTTP
  Range, media serving, and named-client interoperability remain pending.
- Experimental bounded DLNA synchronous TCP MediaRenderer adapter with
  read-timeout, complete-request, response-size, and one-request connection
  cleanup coverage; asynchronous multi-client serving remains outside the
  protocol crate.
- Experimental bounded DLNA single-range media response policy and DIDL-Lite
  item generator with XML escaping and explicit resource limits; streaming
  file backends, full metadata parsing, and GENA remain pending.
- Experimental bounded DLNA GENA subscription/renew/unsubscribe policy with
  deterministic SIDs, timeout bounds, sequenced property events, and capped
  caller-drained queues; lease expiry and callback HTTP remain outside the
  crate.
- Experimental `frameark-airplay` RTSP/CSeq and strict RTP audio packet
  foundation; pairing, decryption, decoding, and Apple interoperability remain
  unimplemented.
- Experimental bounded RAOP ANNOUNCE SDP parser and RTSP session state machine
  covering OPTIONS, ANNOUNCE, SETUP, RECORD, FLUSH, GET_PARAMETER, and
  TEARDOWN; pairing, plist/FairPlay, encryption, timing, decoding, and Apple
  interoperability remain pending.
- Experimental bounded RAOP RTP jitter buffer with sequence reordering,
  wrap-aware late/duplicate handling, explicit gap recovery, and packet-count
  limits; encrypted audio, concealment, decoding, and long-run timing remain
  pending.
- Experimental bounded XML and binary Property List parser/encoder for scalar,
  data, array, and dictionary metadata with duplicate-key, entity, object
  reference, cycle, depth, and size limits; pairing and FairPlay remain
  unsupported.
- Experimental bounded AirPlay mirror H.264 Annex-B/AVCC access-unit contract
  with keyframe detection, orientation validation, and 90 kHz A/V clock bounds;
  transport, decoder, mirror audio, loss recovery, and device compatibility
  remain pending.
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
