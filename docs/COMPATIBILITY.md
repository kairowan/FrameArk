# Compatibility matrix

FrameArk is currently pre-alpha. No protocol or device combination is Stable yet. This file records verified behavior once executable receivers and senders exist; roadmap intent belongs in [PLAN.md](../PLAN.md).

## Maturity labels

| Label | Meaning |
|---|---|
| Stable | Covered by regression tests and a named device/client matrix with version compatibility commitments |
| Beta | Main path works, but device coverage, recovery, or performance remains incomplete |
| Experimental | Disabled or opt-in; behavior and interfaces may change |
| Conditional | Depends on device hardware, drivers, permissions, codecs, or display chain |
| Unsupported | Outside project promises, including DRM or HDCP bypass |

## Current status

| Protocol or capability | Current status | Evidence |
|---|---|---|
| FrameArk DNS-SD discovery (`_frameark._udp.local.`) | Experimental | `frameark-discovery` validation/event-mapping tests plus bounded CLI browse command; multicast smoke coverage is host-dependent |
| FrameArk Native Protocol control plane | Experimental | `frameark-transport` pairing/capability tests, `frameark-native` fake-backend Offer/Start/Status/Stop lifecycle, CLI connect/status/end commands, runnable `framearkd` reference receiver, and opt-in bounded stage diagnostics export; no platform matrix yet |
| FrameArk diagnostics recorder | Experimental | Bounded redaction-safe lifecycle/error ring, deterministic support-text export, control-character normalization, and session-identity omission tests; no file persistence, telemetry, or user-consent UI |
| FrameArk Native encoded media contract | Experimental | `frameark-media` bounded packet and backpressure unit tests; no platform decoder yet |
| FrameArk Native media-frame wire | Experimental | `frameark-native::media_wire` exact-boundary round-trip and malformed-input tests; no platform decoder yet |
| FrameArk Native QUIC media stream | Experimental | `frameark-transport` bounded multi-frame stream tests; no platform decoder or congestion policy yet |
| FrameArk Native renderer adapter | Experimental | `frameark-native::media_session` fake video/audio renderer lifecycle plus `NativeEncodedMediaReceiver` sink integration tests; no Android or desktop decoder yet |
| FrameArk trust registry | Experimental | `frameark-trust` bounded fingerprint validation, explicit revoke, capacity, identity-change rejection, redacted diagnostics, deterministic `FTR1` snapshot round trips, and flushed temporary-file replacement; no platform secure directory, signature verification, or user approval yet |
| Android H.264/PCM renderer adapters | Experimental | `VideoDecoderRenderer`, `AudioOutputRenderer`, bounded `MediaPlaybackPump` dispatch, fixed-rate `MediaPlaybackLoop`, and JVM lifecycle tests; no real-device playback matrix yet |
| Android Rust/JNI receiver lifecycle | Experimental | `frameark-ffi` protected core-session lifecycle, bounded FANP QUIC listener with ephemeral pairing material, `JniMediaQueueSink`, bounded video/audio JNI queue, Kotlin endpoint snapshot/service-gate tests, foreground-service loop ownership, user-visible temporary pairing notification, `ReceiverSessionCoordinator` failure/cleanup tests, and Keystore identity startup gate; no physical-device JNI load or hardware renderer callback yet |
| DLNA/UPnP MediaRenderer | Experimental | `frameark-dlna` bounded SSDP/UDP with one-shot IPv4 multicast `alive/byebye`, bounded synchronous TCP HTTP accept batches, device/SCPD, SOAP AVTransport/RenderingControl/ConnectionManager, deterministic configurable format capability report, in-memory and explicit file-backed single-range media responses, DIDL-Lite generation, GENA policy with monotonic lease expiry, `NOTIFY` encoding, and loopback callback-client tests; no concurrent production scheduler, HTTPS/TLS adapter, URL fetching, format probing, or named-client matrix |
| AirPlay/RAOP audio | Experimental | `frameark-airplay` bounded RTSP/RTP, caller-owned UDP RTP socket parsing, peer/SSRC/payload-filtered `RaopAudioCoordinator`, sequence-aware jitter buffer, ANNOUNCE SDP, XML and binary plist, and RAOP session lifecycle tests; no pairing, FairPlay, encryption, decoder, long-run timing, or Apple-device matrix |
| AirPlay screen mirroring | Experimental | Bounded `MirrorRtspTcpServer` one-request TCP adapter plus mirror RTSP OPTIONS/SETUP/RECORD lifecycle with atomic streaming SETUP reconfiguration, UDP transport/config validation, caller-owned bounded RTP socket parsing, peer/SSRC-filtered `MirrorMediaCoordinator`, H.264 Annex-B/AVCC plus single-NAL/STAP-A/FU-A RTP video assembly, AAC/PCM16 access-unit, mirror-audio RTP reorder/loss policy, keyframe, orientation, reset, and 90 kHz clock tests; no Apple pairing, encryption, decoder, or device matrix |
| Cast V2 compatibility | Planned/Experimental | No executable implementation yet |
| Miracast/Wi-Fi Display | Planned/Conditional | No executable implementation yet |
| FairPlay, Widevine, HDCP bypass | Unsupported | Intentionally outside project scope |

## Evidence required for an entry

Each tested entry must identify receiver build/commit, sender device and OS, network type, codec and media mode, expected and observed result, session duration, diagnostics reference, and limitations. Do not submit private media, credentials, persistent device identifiers, or unsanitized packet captures.
