# FrameArk roadmap status

This is an evidence-based status snapshot, not a claim that the M0–M5 exit
gates have all passed. A milestone is only closed when its `PLAN.md` exit
condition has a repeatable compatibility, performance, and recovery record.
Current compatibility labels remain in [`COMPATIBILITY.md`](COMPATIBILITY.md).

| Milestone | Evidence already merged | Exit gate still open | Status |
|---|---|---|---|
| M0 工程与规范基础 | Rust workspace, Android shell, licenses/governance, CI/security gates, ADR series, repository policy checks, Android native-package verification | No third-platform runtime/load evidence and no final multi-platform release baseline | Partial |
| M1 Native 纵向闭环 | mDNS publication/browse, QUIC/TLS temporary pairing, capability negotiation, FANP control/media, fixture CLI, `framearkd`, loopback H.264+Opus delivery, FAMF/FAMC boundaries, bounded diagnostics | Reference-LAN repeatability with a real hardware decoder and 1080p30 playback is not recorded; decoder and sender remain platform fixtures | Partial |
| M2 Android Receiver MVP | Foreground service, pairing display, multicast-lock lifecycle, Keystore identity gate, JNI queue/package gate, Kotlin bounded config dispatch, explicit recovery seam | Android settings/trust UI, automatic network observer, Android/Desktop sender, physical 1080p60 one-hour stability and short-disconnect evidence | Partial |
| M3 DLNA/media push | SSDP, bounded HTTP/SOAP, AVTransport/RenderingControl/ConnectionManager, GENA leases/NOTIFY, DIDL-Lite, Range/file resources, URI authority validation | Named Android/iOS/desktop controller matrix, two-hour Seek run, DNS-aware URL policy, HTTPS/TLS callback and format probing | Partial |
| M4 RAOP/AirPlay media | Bounded RTSP/RTP, RAOP audio/session contracts, plist/SDP, jitter/loss policy, controls/metadata, HLS playlist safety boundary | Apple pairing/verification, encrypted/FairPlay media, decoder/output, long-run sync/reconnect and named Apple matrix | Partial |
| M5 AirPlay screen mirror | Mirror RTSP lifecycle, UDP/RTP filtering, H.264 assembly, AAC/PCM audio, orientation, 90 kHz clock, loss/reconfiguration tests, explicit H.265 rejection | H.265 assembly, Apple pairing/encryption, decoder/output, iPhone/iPad/Mac Beta matrix and DRM/AWDL messaging | Partial |

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
