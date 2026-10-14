# Compatibility matrix

FrameArk is currently pre-alpha. No protocol or device combination is Stable yet. This file records verified behavior once executable receivers and senders exist; roadmap intent belongs in [PLAN.md](../PLAN.md).

## Maturity labels

Optional Android Sender identity remains **Experimental**. P-256 Keystore
signing now reaches Rust verification and Receiver approval/trust matching;
Ed25519 v1 remains supported. Same-process disposable-emulator JNI/QUIC proof
tests are not a physical-device matrix. See [identity operations](testing/ANDROID_SENDER_IDENTITY.md)
and [evidence](evidence/2026-10-14-sender-identity/README.md). Live revocation,
key attestation/hardware-security guarantees and automatic enrollment are absent.

| Label | Meaning |
|---|---|
| Stable | Covered by regression tests and a named device/client matrix with version compatibility commitments |
| Beta | Main path works, but device coverage, recovery, or performance remains incomplete |
| Experimental | Disabled or opt-in; behavior and interfaces may change |
| Conditional | Depends on device hardware, drivers, permissions, codecs, or display chain |
| Unsupported | Outside project promises, including DRM or HDCP bypass |

## Current status

Android Sender discovery is Experimental: foreground mDNS `boot=cert1` hints,
bounded local-IPv4 candidates, explicit full TLS fingerprint comparison and PIN.
No automatic trust/connection, IPv6, or older-certificate-extension compatibility
is claimed. Manual tickets remain supported. Windows real mDNS/private-IPv4 QUIC
control integration is not two-device Wi-Fi/hardware acceptance; see
[ADR 0124](adr/0124-android-sender-discovery.md).

Android trust mutation is enforced as stopped-only in Rust, including the
listener join interval ([ADR 0118](adr/0118-native-trust-lifecycle-gate.md)).
Regression evidence covers active revoke rejection without snapshot changes
and successful mutation after shutdown, not live revocation of an active peer.
User-run device checks follow the [manual APK checklist](testing/ANDROID_MANUAL_TESTING.md);
no physical-device result is assumed from package delivery alone.

| Protocol or capability | Current status | Evidence |
|---|---|---|
| Android reference Sender | Experimental | Foreground 30-second 720p30/1080p30 EGL/H.264 baseline + raw AAC-LC test-pattern/tone source; pinned QUIC through Rust Native with exact ticket/fingerprint confirmation and separate PIN. Synthetic mode still cancels on pause. No discovery/proof-required pairing, IPv6 or physical/audible-quality claim. [Reference guide](testing/ANDROID_REFERENCE_SENDER.md) |
| Android screen Sender | Experimental | Android 10+ one-use MediaProjection foreground service; optional permitted AudioPlaybackCapture, H.264 + raw AAC-LC through Rust, bounded landscape/portrait canvas, requested 30/60 fps, explicit stop/revocation/lock cleanup. Two-emulator OS capture verifies decoded red/blue pixels, non-silent captured PCM, video-only mode and negotiated landscape/portrait changes with retained audio. Matching Receiver required. No measured hardware 1080p60, one-hour stability, reconnect, microphone, protected-media bypass or Apple support claim. [Capture guide](testing/ANDROID_SCREEN_CAPTURE.md) |
| FrameArk DNS-SD discovery (`_frameark._udp.local.`) | Experimental | `frameark-discovery` validation/event-mapping tests plus bounded CLI browse command; multicast smoke coverage is host-dependent |
| Cross-platform Rust core build matrix | Experimental | Required CI matrix runs formatting, workspace tests, `framearkd` build, and a bounded startup smoke on `windows-latest` and `macos-latest`; this is not media/session runtime, signing, hardware decoder, or device compatibility evidence |
| FrameArk Native Protocol control plane | Experimental | `frameark-transport` pairing/capability tests, loopback Ed25519 challenge/proof verification with fresh nonce and a registry-enrollment integration fixture, canonical proof-key fingerprint bridge into `frameark-trust` after caller approval, exact pre-enrolled proof identity auto-admission on the Rust listener reconnect loop with rotated-key and renamed-device rejection tests, caller-owned post-PIN approval callback with pre-accept rejection tests, `frameark-native` fake-backend Offer/Start/Status/Stop lifecycle, three-cycle real pinned-QUIC loopback media teardown/restart evidence, CLI connect/status/end commands, required CI Linux `framearkd` listener smoke, runnable `framearkd` reference receiver, and opt-in bounded stage diagnostics export; no physical platform sender matrix or live trust revocation; opt-in Android Keystore proof UX is available |
| FrameArk diagnostics recorder | Experimental | Bounded redaction-safe lifecycle/error ring, deterministic support-text export, control-character normalization, and session-identity omission tests; no file persistence, telemetry, or user-consent UI |
| FrameArk Native encoded media contract | Experimental | `frameark-media` bounded packet and backpressure unit tests; no platform decoder yet |
| FrameArk Native media-frame wire | Experimental | `frameark-native::media_wire` exact-boundary round-trip and malformed-input tests; no platform decoder yet |
| FrameArk Native QUIC media stream | Experimental | `frameark-transport` bounded multi-frame stream tests; no platform decoder or congestion policy yet |
| FrameArk Native renderer adapter | Experimental | `frameark-native::media_session` lifecycle/sink tests and clock-preserving Android FAPE integration; actual H.264/AAC platform output on API 34 emulator; no desktop decoder or physical matrix yet |
| FrameArk trust registry | Experimental | `frameark-trust` bounded fingerprint validation and raw-key SHA-256 derivation, explicit revoke, capacity, identity-change rejection, redacted diagnostics, deterministic `FTR1` snapshot round trips, flushed temporary-file replacement, ephemeral expiring `PendingTrustRequests` approval queue, Android Keystore AES-GCM app-private snapshot adapter tests, experimental Android approve/revoke surface, and Rust listener exact-match admission for pre-enrolled proof identities; first approval remains explicit and registry changes are applied between receiver lifecycles |
| Android H.264/AAC/Opus output | Experimental | Ordered FAPE config/media/reset, Rust microsecond pacing, H.264 TextureView rendering and AAC-LC PCM writes verified through real QUIC on API 34 x86_64 emulator; video-only reconfiguration and Receiver rotation preserve audio/timeline/Surface, with bounded aspect-fit output. Mono/stereo AAC-LC and constrained 48 kHz Opus implementations, bounded PCM partial-write handling, synchronized stop and default-network identity filtering. Opus runtime, physical hardware/rotation, audible quality, lip sync, sustained 1080p and background continuation remain unverified/unsupported; see [rotation evidence](evidence/2026-10-14-video-reconfiguration/README.md) |
| Android Rust/JNI receiver lifecycle | Experimental | `frameark-ffi` protected core-session lifecycle, foreground proof allow/deny UI with exact request binding, native expiry queries, pause dismissal, 750ms delayed-approval QUIC regression, cancellation-safe single-use Rust queue, Keystore alias/fingerprint JNI identity bridge with fail-closed startup, Rust-owned bounded trust/revoke/query/snapshot JNI bridge including proof-key fingerprint enrollment, Android Keystore AES-GCM app-private snapshot adapter, experimental UI controller for explicit approve/revoke mutations with proof-key snapshot rollback, bounded Kotlin `VerifiedPeerApprovalQueue` for copied proof keys, TTL/capacity/replay guards and explicit approve/reject outcomes, caller-owned FANP post-PIN approval boundary, optional Android one-shot post-PIN approval gate and JVM tests, real loopback FANP QUIC/TLS temporary pairing/capability/Offer → Start → H.264+Opus media → Stop integration evidence, pre-enrolled proof identity auto-admission on the Rust reconnect loop with exact-key rejection tests, CI verification that debug/release APKs package a non-empty `arm64-v8a/libframeark_ffi.so`, bounded listener with ephemeral pairing material, best-effort `_frameark._udp.local.` publication without secrets, Android Wi-Fi multicast-lock lifecycle, JNI endpoint snapshot and standby pairing display, `JniMediaQueueSink`, bounded video/audio JNI queue, versioned `FAMC` video/audio configuration envelopes and Kotlin bounds validation before frame dispatch, non-destructive `ReceiverSessionCoordinator.onNetworkChanged()` snapshot refresh with real-JNI continuity tests, coalesced `ConnectivityManager` notifications and real QUIC two-one-second-blackout/deadline/cancellation tests, Kotlin endpoint snapshot/service-gate tests, foreground-service loop ownership, user-visible temporary pairing notification, `ReceiverSessionCoordinator` failure/cleanup tests, Keystore identity startup gate, optional proof-required Rust listener mode with a bounded `FAPR` event queue, proof-mode loopback approval before FANP acceptance, and Kotlin `FrameArkNative` proof-event/decision tests; Android/Desktop sender integration, live trust revocation and proof-capable sender UX, physical-device JNI load, hardware renderer callback, and network-switch soak evidence remain open |
| DLNA/UPnP MediaRenderer | Experimental | `frameark-dlna` bounded SSDP/UDP with one-shot IPv4 multicast `alive/byebye`, bounded synchronous TCP HTTP accept batches plus time/request-budgeted non-blocking scheduler, device/SCPD, SOAP AVTransport/RenderingControl/ConnectionManager with duration-bounded `REL_TIME` Seek, deterministic configurable format capability report, in-memory and explicit file-backed single-range media responses, DIDL-Lite generation, GENA policy with monotonic lease expiry, `NOTIFY` encoding, hardened media-URI authority validation, DNS-aware caller-injected public/local address policy, loopback callback-client tests, caller-injected HTTP/HTTPS stream connector, explicit-root Rustls HTTPS media connector with SNI/timeouts and local TLS fixture evidence, bounded payload-free DLNA replay traces, bounded response format probe plus Range-based caller-owned format-probe connector, bounded caller-owned remote media range fetch with exact `206 Content-Range` checks, and bounded known-length contiguous multi-range streaming; no redirect following, decoder inspection, or named-client matrix |
| AirPlay/RAOP audio | Experimental | `frameark-airplay` caller-owned `RaopRtspTcpServer` with bounded single-request and time/request-budgeted scheduling, pre-request caller-owned peer authorization, bounded RTSP/RTP, caller-owned UDP RTP socket parsing, peer/SSRC/payload-filtered `RaopAudioCoordinator`, sequence-aware jitter buffer, ordered unencrypted PCM16 network-to-platform endian conversion, ANNOUNCE SDP, redaction-safe clear/AES-CTR/other-protection classifier, XML and binary plist, bounded `SET_PARAMETER`/`GET_PARAMETER` volume/progress/text metadata, HLS media-playlist parsing, caller-owned HTTP/HTTPS HLS playlist/segment connectors with bounded response/MIME/content-length checks and no redirects, explicit-root Rustls HLS connector with SNI/timeouts and local TLS fixture evidence, bounded payload-free AirPlay replay traces, deterministic bounded parser fuzz-smoke coverage, and optional bounded FIFO segment cache with duplicate replacement and oldest-entry eviction; no adaptive/persistent cache policy, pairing, FairPlay/AES-CTR decryption, ALAC/AAC decoder, long-run timing, or Apple-device matrix |
| AirPlay screen mirroring | Experimental | Bounded `MirrorRtspTcpServer` one-request TCP adapter plus time/request-budgeted non-blocking scheduler, mirror RTSP OPTIONS/SETUP/RECORD lifecycle with atomic streaming SETUP reconfiguration, UDP transport/config validation, caller-owned bounded RTP socket parsing, peer/SSRC-filtered `MirrorMediaCoordinator`, explicit H.264/H.265 codec gate, H.264 Annex-B/AVCC plus H.265 Annex-B and single-NAL/STAP-A/FU-A or AP/FU RTP video assembly, AAC/PCM16 access-unit, mirror-audio RTP reorder/loss policy, keyframe, orientation, reset, and 90 kHz clock tests, and redaction-safe `MirrorCompatibilityReport` admission diagnostics for clear-IP, AWDL, FairPlay/DRM, unknown transport, and pairing states; H.265 remains conditional and there is no Apple pairing, encryption, decoder, or device matrix |
| Cast V2 compatibility | Planned/Experimental | No executable implementation yet |
| Miracast/Wi-Fi Display | Planned/Conditional | No executable implementation yet |
| FairPlay, Widevine, HDCP bypass | Unsupported | Intentionally outside project scope |

## Native video reconfiguration — Experimental

Opcode 5 explicitly negotiates ordered video updates, opcode 6 applies bounded
H.264 geometry/rate after old-stream FIN and before new-stream keyframe. Fixed
opcode 1 is unchanged; unsupported/older receivers fail before media, without
fallback. Android capture requires matched APKs; one consent/VirtualDisplay is
retained, no second capture grant is silently acquired. Rust tests cover exact
ACK, sequence/replay, unsupported and policy rejection, update exhaustion,
missing keyframe, backend failure, timeout and cancellation. API 34 two-emulator
OS rotation/output evidence is not API 29–33 fallback, app-window resize,
physical decoder, zero-gap audio, latency or sustained-FPS acceptance. See
[ADR 0128](adr/0128-native-video-reconfiguration.md).

## Receiver-local audio controls — Experimental

Per-track 0..100 linear gain, separate persistent mute, slider/±10/D-pad controls
and safe output teardown on rejected gain are implemented. API 34 x86_64 tests
exercise real AudioTrack and live H.264/AAC QUIC playback; JVM tests cover bounds,
gain-before-play and failure cleanup. No system volume change, sender network
remote-control command or DLNA/RAOP volume binding is added. Physical audible
quality/mute latency, TV focus, Bluetooth/HDMI and other API levels remain
unverified. See [evidence](evidence/2026-10-14-audio-controls/README.md) and
[manual test](testing/ANDROID_AUDIO_CONTROLS.md).

## Evidence required for an entry

Each tested entry must identify receiver build/commit, sender device and OS, network type, codec and media mode, expected and observed result, session duration, diagnostics reference, and limitations. Use the [device compatibility evidence template](compatibility/device-matrix.md) for new records. Do not submit private media, credentials, persistent device identifiers, or unsanitized packet captures.

The DLNA parser boundary also has deterministic fuzz-smoke coverage for HTTP,
SSDP, and media-range inputs; this is parser-hardening evidence, not named
controller or long-duration playback evidence.
