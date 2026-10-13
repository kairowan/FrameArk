# FrameArk（帧舟）

> Every stream finds a screen.<br>
> 让每一道媒体流，都能抵达一块屏幕。

FrameArk is a Rust-powered, cross-platform screen mirroring and media receiver. It is designed as a reusable receiver core and product ecosystem rather than a single Android application.

FrameArk 是一个以 Rust 为核心的跨平台投屏与媒体接收框架。它的目标不是只做一个 Android APK，而是建立可复用的接收器核心、自有投屏协议、跨平台客户端、SDK 与开发者工具生态。

## Project status / 项目状态

**Pre-alpha: M0–M5 experimental foundations, not completed milestones.** Native
Receiver has foreground Android H.264/AAC output. Sender offers both a
30-second generated reference stream and explicit Android 10+ screen sharing
with optional permitted playback audio over pinned QUIC. Physical performance,
one-hour stability and recovery gates remain open. DLNA and AirPlay remain
bounded protocol foundations, not Apple-device interoperability claims.

**实验性预览：Native 播放与真实屏幕采集开发中。** Sender 保留 30 秒测试流，
新增 Android 10+ 系统授权屏幕共享，可选捕获允许共享的播放音频（不采集麦克风）。
真机结果由设备持有人自行安装并反馈，不把模拟器或请求 60 fps 等同真机验收。
操作见[参考测试流](docs/testing/ANDROID_REFERENCE_SENDER.md)与[屏幕共享说明](docs/testing/ANDROID_SCREEN_CAPTURE.md)。

按里程碑的证据与未满足退出条件见 [`docs/STATUS.md`](docs/STATUS.md)；所有
M0–M5 能力目前仍按兼容性矩阵标记为 Experimental/Conditional，未把真机或
Apple 设备尚未验证的行为写成完成。

See [PLAN.md](PLAN.md) for the complete product definition, architecture, milestones, quality gates, and release criteria.

Maintainers: [protected dual-APK release signing](docs/ANDROID_RELEASE_SIGNING.md)
documents environment secrets, owner approval, signed artifacts and the debug-to-release migration boundary.

## Product direction / 产品方向

FrameArk is built around three long-term pillars:

1. A reusable Rust core for discovery, transport, sessions, timing, trust, diagnostics, and media coordination.
2. Tested interoperability with AirPlay/RAOP and DLNA/UPnP.
3. FrameArk Native Protocol (FANP), an open and versioned low-latency protocol controlled by the project.

Planned products include:

- `frameark-core`: cross-platform Rust core;
- `framearkd`: headless receiver daemon;
- FrameArk Receiver for Android and desktop;
- FrameArk Sender for Android, Windows, Linux, macOS, and browsers;
- `frameark-sdk`, `frameark-cli`, `frameark-lab`, and Web Admin. The current
  CLI is an experimental discovery/lifecycle and fixture tool documented in
  [`tools/frameark-cli/README.md`](tools/frameark-cli/README.md).

## Protocol roadmap / 协议路线

| Area | Target position | Important boundary |
|---|---|---|
| FrameArk Native Protocol | Primary, formally versioned protocol | Local-first QUIC/TLS transport |
| DLNA/UPnP | Stable target | Compatibility is verified against named clients |
| AirPlay/RAOP | Stable target for tested scenarios | No FairPlay or universal AWDL claim |
| Cast V2 | Compatibility mode | Not a certified Chromecast replacement |
| Miracast/Wi-Fi Display | Conditional/experimental | Requires platform and device sink support |
| DRM/HDCP bypass | Unsupported | The project does not bypass protected content |

No capability is called Stable until it has a documented compatibility matrix, automated regression coverage, and explicit limitations. The initial matrix lives in [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## Architecture / 架构

```text
Apps and UI
    ↓
Platform adapters and FFI
    ↓
Media sessions, clock, buffers, and codec model
    ↓
Protocol adapters
    ↓
Transport and discovery
    ↓
Identity, trust, configuration, events, and metrics
```

Rust owns protocol and cross-platform state. Kotlin, Swift, desktop, and web layers integrate operating-system APIs, capture, hardware codecs, rendering, audio routing, lifecycle, and UI.

The current M1/M3 workspace includes the `frameark-discovery` mDNS/DNS-SD
prototype, the experimental `frameark-transport` FANP QUIC/TLS pairing and
bounded media stream, the `frameark-native` control/media adapters, and the
`frameark-cli` fixture sender. Discovery publishes and browses the reserved
`_frameark._udp.local.` service type with bounded metadata and automatic
address tracking. Transport pins an ephemeral certificate, exchanges a
temporary six-digit pairing code, negotiates the shared core capability set,
and carries bounded FAM1 access units. Rust can now exercise one complete
Offer → Start → media → Stop loopback with fake renderers, and the Android
shell forwards its foreground lifecycle to a protected Rust session through
JNI. FANP can also forward received encoded access units through
`NativeEncodedMediaReceiver` to the bounded `JniMediaQueueSink`; Android's
`MediaPlaybackPump` and `MediaPlaybackLoop` provide bounded polling and
fixed-rate lifecycle scheduling while platform consumers own the subsequent
codec and render APIs. Neither component grants persistent trust, ships a real
codec, or claims Android hardware playback yet. The `frameark-dlna` crate additionally exposes bounded SSDP and
caller-owned HTTP/SOAP renderer contracts and one-shot IPv4 SSDP multicast
announcements. GENA events can now be delivered by
an explicitly plain-HTTP, bounded callback client plus a caller-injected
HTTP/HTTPS stream boundary; built-in TLS, URL fetching, streaming media
serving, and named-client interoperability remain future work.

The executable FANP profile is documented in
[docs/protocols/fanp/README.md](docs/protocols/fanp/README.md), with current
limits in [docs/compatibility/fanp-transport.md](docs/compatibility/fanp-transport.md).

The AirPlay mirror contracts include bounded RTSP setup, H.264 video and
AAC/PCM16 audio access units, plus sequence-aware mirror video/audio RTP
pipelines that expose explicit loss recovery to the caller. Pairing, encryption,
decoder/output integration, socket ownership, and Apple-device compatibility
remain Experimental.

## Development workflow / 开发流程

FrameArk does not accept direct development on `main`.

```powershell
git fetch origin
git switch -c feat/example origin/main
python .codex/skills/frameark-development/scripts/guard_branch.py --repo .
```

Every completed template, feature, protocol increment, or independent fix must pass its relevant checks and receive an atomic [Conventional Commit](https://www.conventionalcommits.org/). Changes reach `main` only through reviewed pull requests and required CI/security checks.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before starting. Repository-specific agent guidance is in [AGENTS.md](AGENTS.md), and the reusable development Skill is in [.codex/skills/frameark-development](.codex/skills/frameark-development).

### Release artifacts / 发布产物

Reviewed release tags publish a source archive, the verified Android release
APK when the Android product is present, a deterministic SPDX 2.3 SBOM, and
`SHA256SUMS` covering every published file. The release workflow also emits
GitHub OIDC build-provenance attestations for every file in `dist/`. The SBOM
describes the locked Rust workspace; platform SDKs, optional codecs, and
platform signing remain separate release-review concerns. Pre-alpha artifacts
must not be treated as a Stable compatibility claim.

## Security and privacy / 安全与隐私

- local-network operation without a required account or cloud service;
- no telemetry by default;
- bounded parsing and resource use for untrusted protocol data;
- platform-backed storage for long-term private keys;
- explicit permission for remote input, clipboard, file transfer, or internet relay;
- redacted logs and opt-in diagnostics.

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Contributing / 参与贡献

Issues and pull requests are welcome while the project is being bootstrapped. Please use the repository templates, keep changes focused, include tests and documentation, and follow the branch and commit policy in [CONTRIBUTING.md](CONTRIBUTING.md).

Project decisions and maintainer responsibilities are documented in [GOVERNANCE.md](GOVERNANCE.md). Community behavior is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License / 开源协议

Unless explicitly stated otherwise, FrameArk is licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE); or
- [MIT License](LICENSE-MIT),

at your option. See [LICENSE](LICENSE) for details.

Third-party components, optional codec backends, protocol trademarks, and distributed binaries may have additional notices or obligations. They must be reviewed before inclusion or release.

## Trademark and compatibility notice

FrameArk and 帧舟 are project names. AirPlay, Chromecast, Miracast, DLNA, Android, Apple, Google, and other names belong to their respective owners. Their use in this repository describes interoperability only and does not imply endorsement or certification.
