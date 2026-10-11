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
| Android Rust/JNI receiver lifecycle | Experimental | `frameark-ffi` protected core-session lifecycle, Keystore alias/fingerprint JNI identity bridge with fail-closed startup, Rust-owned bounded trust/revoke/query JNI bridge, real loopback FANP QUIC/TLS temporary pairing/capability/Offer → Start → H.264+Opus media → Stop integration evidence, CI verification that debug/release APKs package a non-empty `arm64-v8a/libframeark_ffi.so`, bounded listener with ephemeral pairing material, best-effort `_frameark._udp.local.` publication without secrets, Android Wi-Fi multicast-lock lifecycle, JNI endpoint snapshot and standby pairing display, `JniMediaQueueSink`, bounded video/audio JNI queue, versioned `FAMC` video/audio configuration envelopes and Kotlin bounds validation before frame dispatch, explicit bounded `ReceiverSessionCoordinator.recover()` restart path and lifecycle tests, coalesced `ConnectivityManager` network observer dispatch, Kotlin endpoint snapshot/service-gate tests, foreground-service loop ownership, user-visible temporary pairing notification, `ReceiverSessionCoordinator` failure/cleanup tests, and Keystore identity startup gate; no physical-device JNI load, hardware renderer callback, secure trust persistence, or network-switch soak evidence yet |
| DLNA/UPnP MediaRenderer | Experimental | `frameark-dlna` bounded SSDP/UDP with one-shot IPv4 multicast `alive/byebye`, bounded synchronous TCP HTTP accept batches plus time/request-budgeted non-blocking scheduler, device/SCPD, SOAP AVTransport/RenderingControl/ConnectionManager, deterministic configurable format capability report, in-memory and explicit file-backed single-range media responses, DIDL-Lite generation, GENA policy with monotonic lease expiry, `NOTIFY` encoding, hardened media-URI authority validation, DNS-aware caller-injected public/local address policy, and loopback callback-client tests; no concurrent scheduler, HTTPS/TLS adapter, URL fetching, format probing, or named-client matrix |
| AirPlay/RAOP audio | Experimental | `frameark-airplay` caller-owned `RaopRtspTcpServer` with bounded single-request and time/request-budgeted scheduling, bounded RTSP/RTP, caller-owned UDP RTP socket parsing, peer/SSRC/payload-filtered `RaopAudioCoordinator`, sequence-aware jitter buffer, ordered unencrypted PCM16 network-to-platform endian conversion, ANNOUNCE SDP, XML and binary plist, bounded `SET_PARAMETER`/`GET_PARAMETER` volume/progress/text metadata, HLS media-playlist parsing with URI safety checks, and RAOP session lifecycle tests; no network fetch/cache, pairing, FairPlay, encryption, ALAC/AAC decoder, long-run timing, or Apple-device matrix |
| AirPlay screen mirroring | Experimental | Bounded `MirrorRtspTcpServer` one-request TCP adapter plus time/request-budgeted non-blocking scheduler, mirror RTSP OPTIONS/SETUP/RECORD lifecycle with atomic streaming SETUP reconfiguration, UDP transport/config validation, caller-owned bounded RTP socket parsing, peer/SSRC-filtered `MirrorMediaCoordinator`, explicit H.264/H.265 codec gate, H.264 Annex-B/AVCC plus H.265 Annex-B and single-NAL/STAP-A/FU-A or AP/FU RTP video assembly, AAC/PCM16 access-unit, mirror-audio RTP reorder/loss policy, keyframe, orientation, reset, and 90 kHz clock tests; H.265 remains conditional and has no Apple pairing, encryption, decoder, or device matrix |
| Cast V2 compatibility | Planned/Experimental | No executable implementation yet |
| Miracast/Wi-Fi Display | Planned/Conditional | No executable implementation yet |
| FairPlay, Widevine, HDCP bypass | Unsupported | Intentionally outside project scope |

## Evidence required for an entry

Each tested entry must identify receiver build/commit, sender device and OS, network type, codec and media mode, expected and observed result, session duration, diagnostics reference, and limitations. Do not submit private media, credentials, persistent device identifiers, or unsanitized packet captures.
