# FrameArk roadmap status

This is an evidence-based status snapshot, not a claim that the M0–M5 exit
gates have all passed. A milestone is only closed when its `PLAN.md` exit
condition has a repeatable compatibility, performance, and recovery record.
Current compatibility labels remain in [`COMPATIBILITY.md`](COMPATIBILITY.md).

| Milestone | Evidence already merged | Exit gate still open | Status |
|---|---|---|---|
| M0 工程与规范基础 | Rust workspace, Android shell, licenses/governance, CI/security gates, ADR series, repository policy checks, Android native-package verification, deterministic SPDX SBOM generation, release APK/source/checksum publication, GitHub OIDC provenance attestations for published artifacts | No third-platform runtime/load evidence, platform signing policy, and final multi-platform release baseline | Partial |
| M1 Native 纵向闭环 | mDNS publication/browse, QUIC/TLS temporary pairing, capability negotiation, FANP control/media, fixture CLI, `framearkd`, loopback H.264+Opus delivery, FAMF/FAMC boundaries, bounded diagnostics | Reference-LAN repeatability with a real hardware decoder and 1080p30 playback is not recorded; decoder and sender remain platform fixtures | Partial |
| M2 Android Receiver MVP | Foreground service, pairing display, experimental trust approve/revoke settings UI, bounded Rust `PendingTrustRequests` approval queue with monotonic expiry, caller-owned FANP post-PIN approval callback boundary, multicast-lock lifecycle, Keystore identity gate, alias/fingerprint JNI identity bridge, Rust-owned trust registry JNI bridge, encrypted Android Keystore AES-GCM snapshot adapter with bounded JNI FTR1 export/import, JNI queue/package gate, Kotlin bounded config dispatch, explicit recovery seam, ConnectivityManager observer with coalesced recovery dispatch | Authenticated sender key-possession transcript and pairing-driven Android approval UX, Android/Desktop sender, physical 1080p60 one-hour stability and short-disconnect evidence | Partial |
| M3 DLNA/media push | SSDP, bounded HTTP/SOAP, AVTransport/RenderingControl/ConnectionManager, GENA leases/NOTIFY, DIDL-Lite, Range/file resources, URI authority validation, caller-injected DNS-aware public/local network policy, caller-injected HTTP/HTTPS stream callback boundary, bounded MIME/DLNA response format probe | Named Android/iOS/desktop controller matrix, two-hour Seek run, production TLS connector, URL fetching/redirect policy, and decoder-backed format probing | Partial |
| M4 RAOP/AirPlay media | Bounded RTSP/RTP, RAOP audio/session contracts, plist/SDP, jitter/loss policy, controls/metadata, pre-request caller-owned RAOP/mirror RTSP peer authorization, HLS playlist and segment safety boundaries, caller-owned HTTP/HTTPS HLS playlist/segment connectors with bounded response/MIME/content-length checks and no redirects | Apple pairing/verification, encrypted/FairPlay media, production DNS/TLS policy, cache eviction, decoder/output, long-run sync/reconnect and named Apple matrix | Partial |
| M5 AirPlay screen mirror | Mirror RTSP lifecycle, UDP/RTP filtering, H.264 and conditional H.265 single-NAL/AP/FU assembly, AAC/PCM audio, orientation, 90 kHz clock, loss/reconfiguration tests | Apple pairing/encryption, decoder/output, iPhone/iPad/Mac Beta matrix and DRM/AWDL messaging | Partial |

## Current interpretation

The repository has a substantial M0–M5 experimental foundation, but no M0–M5
milestone is closed because every row still has an exit-gate item above. The
remaining work is primarily real platform interoperability, decoder/output
integration, security protocol implementation, named-device regression, and
long-duration evidence—not a missing README or a silent compatibility claim.

Every new unit continues to follow the repository rule: focused branch,
atomic Conventional Commit, required CI/security checks, reviewed PR, and
administrator Rebase and merge. Local hardware-dependent checks must be
recorded as unavailable rather than replaced with fabricated results.
