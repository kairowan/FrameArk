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
| FrameArk Native Protocol control plane | Experimental | `frameark-transport` pairing/capability tests, `frameark-native` fake-backend Offer/Start/Status/Stop lifecycle, and CLI connect/status/end commands; no platform matrix yet |
| FrameArk Native encoded media contract | Experimental | `frameark-media` bounded packet and backpressure unit tests; no platform decoder yet |
| FrameArk Native media-frame wire | Experimental | `frameark-native::media_wire` exact-boundary round-trip and malformed-input tests; no platform decoder yet |
| FrameArk Native QUIC media stream | Experimental | `frameark-transport` bounded multi-frame stream tests; no platform decoder or congestion policy yet |
| FrameArk Native renderer adapter | Experimental | `frameark-native::media_session` fake video/audio renderer lifecycle plus `NativeEncodedMediaReceiver` sink integration tests; no Android or desktop decoder yet |
| Android H.264/PCM renderer adapters | Experimental | `VideoDecoderRenderer`, `AudioOutputRenderer`, bounded `MediaPlaybackPump` dispatch, and JVM config validation tests; no real-device playback matrix yet |
| Android Rust/JNI receiver lifecycle | Experimental | `frameark-ffi` protected core-session lifecycle, `JniMediaQueueSink`, bounded video/audio JNI queue, and Kotlin envelope tests; no physical-device JNI load, Android service network attachment, or hardware renderer callback yet |
| DLNA/UPnP MediaRenderer | Experimental | `frameark-dlna` bounded SSDP/UDP with one-shot IPv4 multicast `alive/byebye`, synchronous TCP HTTP device/SCPD, SOAP AVTransport/RenderingControl/ConnectionManager, single-range fixture, DIDL-Lite generation, GENA policy, `NOTIFY` encoding, and loopback callback-client tests; no multicast scheduler, HTTPS/TLS adapter, lease expiry, streaming media backend, or named-client matrix |
| AirPlay/RAOP audio | Experimental | `frameark-airplay` bounded RTSP/RTP, sequence-aware jitter buffer, ANNOUNCE SDP, XML and binary plist, and RAOP session lifecycle tests; no pairing, FairPlay, encryption, decoder, long-run timing, or Apple-device matrix |
| AirPlay screen mirroring | Experimental | Bounded mirror RTSP OPTIONS/SETUP/RECORD lifecycle, UDP transport/config validation, H.264 Annex-B/AVCC and AAC/PCM16 access-unit, mirror-audio RTP reorder/loss policy, keyframe, orientation, and 90 kHz clock tests; no Apple pairing, encryption, decoder, socket ownership, reconfiguration recovery, or device matrix |
| Cast V2 compatibility | Planned/Experimental | No executable implementation yet |
| Miracast/Wi-Fi Display | Planned/Conditional | No executable implementation yet |
| FairPlay, Widevine, HDCP bypass | Unsupported | Intentionally outside project scope |

## Evidence required for an entry

Each tested entry must identify receiver build/commit, sender device and OS, network type, codec and media mode, expected and observed result, session duration, diagnostics reference, and limitations. Do not submit private media, credentials, persistent device identifiers, or unsanitized packet captures.
