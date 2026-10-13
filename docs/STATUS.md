# FrameArk roadmap status

This is an evidence-based status snapshot, not a claim that the M0–M5 exit
gates have all passed. A milestone is only closed when its `PLAN.md` exit
condition has a repeatable compatibility, performance, and recovery record.
Current compatibility labels remain in [`COMPATIBILITY.md`](COMPATIBILITY.md).

**Repository control drift, audited 2026-10-13:** the repository is now private;
main protection is unavailable through the API (403), dependency review is
unsupported, and CodeQL upload reports code scanning disabled. The signing
environment no longer returns required reviewers; no environment signing secrets
were configured. Merges and production signing setup are blocked pending the
owner's visibility/hosting decision and verified recovery. Existing workflow code
does not prove these external controls are effective. See the
[protection audit](BRANCH_PROTECTION.md).

| Milestone | Evidence already merged | Exit gate still open | Status |
|---|---|---|---|
| M0 工程与规范基础 | Rust workspace, Android shell, licenses/governance, CI/security gates including required `framearkd` Linux listener smoke, Windows/macOS Rust workspace format/test/build matrix and bounded `framearkd` startup smoke, ADR series, repository policy checks, Android native-package verification, deterministic SPDX SBOM generation, release APK/source/checksum publication, GitHub OIDC provenance attestations for published artifacts | Non-Linux media/session runtime evidence beyond startup, platform signing policy, and final multi-platform release baseline | Partial |
| M1 Native 纵向闭环 | mDNS publication/browse, QUIC/TLS temporary pairing, capability negotiation, FANP control/media, fixture CLI, `framearkd`, loopback H.264+Opus delivery, repeated loopback teardown/restart, clock-preserving FAPE JNI playback, foreground Android H.264/AAC decode/output, installable reference Sender with ticket/PIN UI, three consecutive two-emulator UI-driven 1080p30 runs with 900 rendered frames each | Physical reference-LAN discovery/authorization/1080p30 hardware playback is not recorded; Sender discovery UI, Opus output and measured sync/performance remain open | Partial |
| M2 Android Receiver MVP | Foreground service, pairing display, foreground Rust-verified proof allow/deny UI with native expiry queries and pause dismissal, delayed-approval QUIC regression and cancellation-safe Rust queue, optional Android one-shot post-PIN approval gate, experimental trust approve/revoke settings UI, bounded Rust `PendingTrustRequests` approval queue with monotonic expiry, bounded Kotlin `VerifiedPeerApprovalQueue` with proof-key copying, TTL/capacity/replay guards and explicit approve/reject outcomes, caller-owned FANP post-PIN approval callback boundary, optional FANP Ed25519 sender challenge/proof verification with Rust-exposed temporary identity, optional proof-required Rust listener with 16-entry `FAPR` event queue and explicit JNI decision before session acceptance, real proof-mode loopback approval evidence, canonical proof-key fingerprint bridge into `frameark-trust`, exact pre-enrolled proof identity auto-admission on the Rust reconnect loop with rotated-key and renamed-device rejection tests, explicit Android JNI/Kotlin raw-key enrollment seam after proof approval, encrypted snapshot rollback in the Android trust controller, multicast-lock lifecycle, Keystore identity gate, alias/fingerprint JNI identity bridge, Rust-owned trust registry JNI bridge, encrypted Android Keystore AES-GCM snapshot adapter with bounded JNI FTR1 export/import, JNI queue/package gate, Kotlin bounded config dispatch, explicit recovery seam, ConnectivityManager observer with coalesced recovery dispatch | Android/Desktop sender integration, live trust revocation and proof-capable sender UX, physical 1080p60 one-hour stability and short-disconnect evidence | Partial |
| M3 DLNA/media push | SSDP, bounded HTTP/SOAP, AVTransport/RenderingControl/ConnectionManager with duration-bounded `REL_TIME` Seek, GENA leases/NOTIFY, DIDL-Lite, Range/file resources, URI authority validation, caller-injected DNS-aware public/local network policy, caller-injected HTTP/HTTPS stream callback boundary, bounded MIME/DLNA response format probe, caller-owned format-probe connector, bounded caller-owned remote media range fetch with exact `206 Content-Range` validation, bounded known-length multi-range streaming with contiguous chunk scheduling, explicit-root Rustls HTTPS media connector with SNI/timeouts and local TLS fixture evidence, and bounded payload-free DLNA replay traces | Named Android/iOS/desktop controller matrix, two-hour Seek run, redirect-follow policy (if ever enabled), and decoder-backed format probing | Partial |
| M4 RAOP/AirPlay media | Bounded RTSP/RTP, RAOP audio/session contracts, plist/SDP, redaction-safe clear/AES-CTR/other-protection classification, jitter/loss policy, controls/metadata, pre-request caller-owned RAOP/mirror RTSP peer authorization, HLS playlist and segment safety boundaries, caller-owned HTTP/HTTPS HLS playlist/segment connectors with bounded response/MIME/content-length checks and no redirects, explicit-root Rustls HLS connector with SNI/timeouts and local TLS fixture evidence, bounded payload-free AirPlay replay trace, and optional bounded FIFO HLS segment cache with byte/entry eviction | Apple pairing/verification, encrypted/FairPlay media, adaptive/persistent cache policy, decoder/output, long-run sync/reconnect and named Apple matrix | Partial |
| M5 AirPlay screen mirror | Mirror RTSP lifecycle, UDP/RTP filtering, H.264 and conditional H.265 single-NAL/AP/FU assembly, AAC/PCM audio, orientation, 90 kHz clock, loss/reconfiguration tests, and redaction-safe `MirrorCompatibilityReport` decisions for clear-IP, AWDL, FairPlay/DRM, unknown transport, and pairing states | Apple pairing/encryption, decoder/output, iPhone/iPad/Mac Beta matrix, and real AWDL/DRM interoperability evidence | Partial |

## Current interpretation

Android release engineering now separates unsigned dual-APK build/test from
protected environment signing and publication. Real disposable-key signing,
certificate pinning, alignment and failure cleanup are exercised in CI. The
`android-release` environment must require owner approval and tag-only deployment,
but approval protection is currently unavailable as noted above; production
secrets are not configured by the agent. Both APKs now embed a verified
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
M2's measured 1080p60, one-hour, dynamic reconfiguration or reconnect gates.
See [capture operations](testing/ANDROID_SCREEN_CAPTURE.md), [ADR 0121](adr/0121-android-screen-capture.md)
and [actual OS capture evidence](evidence/2026-10-13-screen-capture/README.md).

The separate 30-second generated test mode remains available with its
[operator guide](testing/ANDROID_REFERENCE_SENDER.md) and
[reference-stream evidence](evidence/2026-10-13-reference-sender/README.md).
Both modes require explicit ticket/fingerprint/PIN entry; Sender discovery and
peer identity proof remain open. M1 still requires physical reference-LAN
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
