# FrameArk roadmap status

This is an evidence-based status snapshot, not a claim that the M0–M5 exit
gates have all passed. A milestone is only closed when its `PLAN.md` exit
condition has a repeatable compatibility, performance, and recovery record.
Current compatibility labels remain in [`COMPATIBILITY.md`](COMPATIBILITY.md).

**Repository control recovery, audited 2026-10-14:** the owner restored public
visibility. Signing-environment approval/tag controls are again visible through
the API, and missing main branch protection was reapplied and verified. Production
environment secrets remain unset. Fresh successful checks on the current PR/main
commits remain mandatory; restored settings do not imply a successful release.
The earlier private-repository interruption and recovery are recorded in the
[protection audit](BRANCH_PROTECTION.md).

| Milestone | Evidence already merged | Exit gate still open | Status |
|---|---|---|---|
| M0 工程与规范基础 | Rust workspace, Android shell, licenses/governance, CI/security gates including required `framearkd` Linux listener smoke, Windows/macOS Rust workspace format/test/build matrix and bounded `framearkd` startup smoke, ADR series, repository policy checks, Android native-package verification, deterministic SPDX SBOM generation, release APK/source/checksum publication, GitHub OIDC provenance attestations for published artifacts | Non-Linux media/session runtime evidence beyond startup, platform signing policy, and final multi-platform release baseline | Partial |
| M1 Native 纵向闭环 | mDNS publication/browse, bounded Sender discovery/select/explicit-confirmation UI, QUIC/TLS temporary pairing, capability negotiation, FANP control/media, fixture CLI, `framearkd`, loopback H.264+Opus delivery, repeated loopback teardown/restart, clock-preserving FAPE JNI playback, foreground Android H.264/AAC decode/output, installable reference Sender with ticket/PIN UI, three consecutive two-emulator UI-driven 1080p30 runs with 900 rendered frames each | Physical reference-LAN discovery/authorization/1080p30 hardware playback is not recorded; Opus output and measured sync/performance remain open | Partial |
| M2 Android Receiver MVP | Foreground service, pairing display, foreground Rust-verified proof allow/deny UI with native expiry queries and pause dismissal, delayed-approval QUIC regression and cancellation-safe Rust queue, optional Android one-shot post-PIN approval gate, experimental trust approve/revoke settings UI, bounded Rust `PendingTrustRequests` approval queue with monotonic expiry, bounded Kotlin `VerifiedPeerApprovalQueue` with proof-key copying, TTL/capacity/replay guards and explicit approve/reject outcomes, caller-owned FANP post-PIN approval callback boundary, optional FANP Ed25519/P-256 sender challenge/proof verification with opt-in Android Keystore signing with Rust-exposed temporary identity, optional proof-required Rust listener with 16-entry `FAPR` event queue and explicit JNI decision before session acceptance, real proof-mode loopback approval evidence, canonical proof-key fingerprint bridge into `frameark-trust`, exact pre-enrolled proof identity auto-admission on the Rust reconnect loop with rotated-key and renamed-device rejection tests, explicit Android JNI/Kotlin raw-key enrollment seam after proof approval, encrypted snapshot rollback in the Android trust controller, multicast-lock lifecycle, Keystore identity gate, alias/fingerprint JNI identity bridge, Rust-owned trust registry JNI bridge, encrypted Android Keystore AES-GCM snapshot adapter with bounded JNI FTR1 export/import, JNI queue/package gate, Kotlin bounded config dispatch, non-destructive ConnectivityManager endpoint refresh, stable JNI pairing material across notification storms, real QUIC media continuity through two one-second UDP blackouts with timeout/cancellation cleanup tests | Physical sender identity integration, desktop signing, live trust revocation and streamlined persistent enrollment UX, physical 1080p60 one-hour stability and short-disconnect evidence | Partial |
| M3 DLNA/media push | SSDP, bounded HTTP/SOAP, AVTransport/RenderingControl/ConnectionManager with duration-bounded `REL_TIME` Seek, GENA leases/NOTIFY, DIDL-Lite, Range/file resources, URI authority validation, caller-injected DNS-aware public/local network policy, caller-injected HTTP/HTTPS stream callback boundary, bounded MIME/DLNA response format probe, caller-owned format-probe connector, bounded caller-owned remote media range fetch with exact `206 Content-Range` validation, bounded known-length multi-range streaming with contiguous chunk scheduling, explicit-root Rustls HTTPS media connector with SNI/timeouts and local TLS fixture evidence, and bounded payload-free DLNA replay traces | Named Android/iOS/desktop controller matrix, two-hour Seek run, redirect-follow policy (if ever enabled), and decoder-backed format probing | Partial |
| M4 RAOP/AirPlay media | Bounded RTSP/RTP, RAOP audio/session contracts, plist/SDP, redaction-safe clear/AES-CTR/other-protection classification, jitter/loss policy, controls/metadata, pre-request caller-owned RAOP/mirror RTSP peer authorization, HLS playlist and segment safety boundaries, caller-owned HTTP/HTTPS HLS playlist/segment connectors with bounded response/MIME/content-length checks and no redirects, explicit-root Rustls HLS connector with SNI/timeouts and local TLS fixture evidence, bounded payload-free AirPlay replay trace, and optional bounded FIFO HLS segment cache with byte/entry eviction | Apple pairing/verification, encrypted/FairPlay media, adaptive/persistent cache policy, decoder/output, long-run sync/reconnect and named Apple matrix | Partial |
| M5 AirPlay screen mirror | Mirror RTSP lifecycle, UDP/RTP filtering, H.264 and conditional H.265 single-NAL/AP/FU assembly, AAC/PCM audio, orientation, 90 kHz clock, loss/reconfiguration tests, and redaction-safe `MirrorCompatibilityReport` decisions for clear-IP, AWDL, FairPlay/DRM, unknown transport, and pairing states | Apple pairing/encryption, decoder/output, iPhone/iPad/Mac Beta matrix, and real AWDL/DRM interoperability evidence | Partial |

## Current interpretation

Native screen capture now negotiates ordered video-only updates within the same
authenticated QUIC session. Android rebuilds its encoder around one existing
VirtualDisplay; Receiver rebuilds only video and retains its Activity/Surface
across size/orientation changes. Audio, timeline and local gain remain unchanged.
Real two-emulator capture/codec evidence and bounded rejection/cleanup tests are
recorded in [ADR 0128](adr/0128-native-video-reconfiguration.md) and
[rotation evidence](evidence/2026-10-14-video-reconfiguration/README.md).
This closes the initial dynamic-geometry software gap, not physical rotation,
measured 1080p60, one-hour performance, reconnect or an M2 exit gate.

Receiver-local volume/mute now reaches the real AudioTrack output, with bounded
gain, D-pad controls, app-private preferences and gain-before-play on reset.
Live H.264/AAC loopback playback continues during mute/volume/unmute actions.
See [ADR 0127](adr/0127-receiver-local-audio-controls.md) and
[evidence](evidence/2026-10-14-audio-controls/README.md). This closes the local
audio-control software gap, not sender-side remote control, physical loudness,
audio-route compatibility or any M2 exit gate. Dynamic geometry is recorded separately above.

Android default-network notifications no longer tear down the Rust listener or
playback owners. Real QUIC loss tests preserve one authenticated stream through
two one-second blackouts; JVM and real-JNI tests preserve pairing material on
notifications and release it on explicit stop. This is bounded same-connection
continuity, not reconnect tickets, IP/network migration, full-rate capture
recovery or physical M2 acceptance. See [ADR 0126](adr/0126-retain-receiver-on-network-notifications.md)
and [test evidence](evidence/2026-10-14-network-continuity/README.md).

Android Sender now optionally signs fresh FANP challenges with a persistent,
nonexportable P-256 Keystore key for both reference and screen-capture paths.
Rust verifies v2 proofs and exposes the matching raw-key fingerprint to the
existing Receiver approval UI/trust policy. The default-off UI invalidates old
PIN/confirmation when changed; no downgrade/auto-enrollment is added.
See [ADR 0125](adr/0125-android-sender-keystore-proof.md) and the
[identity evidence](evidence/2026-10-14-sender-identity/README.md). This closes
the proof-capable Android Sender software gap, not physical identity/performance,
live revocation, recovery, desktop integration, or M2 acceptance.

Android Sender now has an Experimental foreground mDNS candidate list with
bounded Rust bootstrap parsing/cache and selection into the existing explicitly
confirmed, certificate-pinned PIN flow. Selection is not trust. The opt-in real
Windows mDNS/private-IPv4 QUIC integration reaches negotiation/Start/Stop, not
decoded output or two-device Wi-Fi acceptance. Manual tickets remain available.
See [ADR 0124](adr/0124-android-sender-discovery.md). M1 remains Partial pending
the owner's physical reference-LAN workflow and hardware evidence.

Android release engineering now separates unsigned dual-APK build/test from
protected environment signing and publication. Real disposable-key signing,
certificate pinning, alignment and failure cleanup are exercised in CI. The
`android-release` environment requires owner approval and tag-only deployment,
with configuration recovery verified as noted above; production secrets are not
configured by the agent. Both APKs now embed a verified
third-party notice set, with complete lockfile coverage, actual Android runtime
resolution and original legal texts; identical sidecars are published with them.
This closes the old single-APK/unsigned-filename and missing-notice pipeline defects,
**not** production-key publishing, legal/patent review, desktop signing or a milestone gate.
See [release signing operations](ANDROID_RELEASE_SIGNING.md) and
[ADR 0122](adr/0122-protected-dual-apk-release-signing.md).
Notice coverage and its conservative-superset limits are documented in
[dependency operations](DEPENDENCY_NOTICES.md) and [ADR 0123](adr/0123-verified-distribution-notices.md).

Android Sender now also provides explicit MediaProjection screen capture and
optional AudioPlaybackCapture on Android 10+, through the same Rust Native
session path. A non-exported foreground service owns the one-use OS grant;
notification/UI stop, screen-off and system revocation end capture. Geometry
is bounded to 1080p area in either orientation, 30/60 fps requested, maximum
one-hour session. This closes the initial real-source software gap, **not**
M2's measured 1080p60, one-hour or reconnect gates; dynamic geometry is recorded above.
See [capture operations](testing/ANDROID_SCREEN_CAPTURE.md), [ADR 0121](adr/0121-android-screen-capture.md)
and [actual OS capture evidence](evidence/2026-10-13-screen-capture/README.md).

The separate 30-second generated test mode remains available with its
[operator guide](testing/ANDROID_REFERENCE_SENDER.md) and
[reference-stream evidence](evidence/2026-10-13-reference-sender/README.md).
Both modes require explicit ticket/fingerprint/PIN confirmation; discovery and
optional peer identity proof are described above. M1 still requires physical reference-LAN
discovery and hardware evidence. No M0–M5 exit gate is newly marked complete.

The Android service now drives real platform codecs and outputs instead of
no-op media consumers. A platform-encoded gray H.264/silent AAC probe traverses
pinned loopback QUIC and JNI; see the [playback evidence](evidence/2026-10-13-native-playback/README.md).
This is short, low-resolution emulator evidence, not a completed M1/M2 hardware
gate. PCM tail may be discarded on Stop; long-run drift correction is absent.

The Android trust lifecycle now has a Rust-enforced mutation gate through
listener shutdown; this closes the JNI/settings-policy mismatch, not the live
revocation or playback gates. Device testing is operator-run using the
[manual APK checklist](testing/ANDROID_MANUAL_TESTING.md). APK delivery is not
counted as a successful physical-device test until results are returned.

The repository has a substantial M0–M5 experimental foundation, but no M0–M5
milestone is closed because every row still has an exit-gate item above. The
remaining work is primarily real platform interoperability, decoder/output
integration, security protocol implementation, named-device regression, and
long-duration evidence—not a missing README or a silent compatibility claim.

Every new unit continues to follow the repository rule: focused branch,
atomic Conventional Commit, required CI/security checks, reviewed PR, and
administrator Rebase and merge. Local hardware-dependent checks must be
recorded as unavailable rather than replaced with fabricated results.
