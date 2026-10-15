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
one-hour stability and recovery gates remain open. DLNA now has an opt-in
Android HTTP Range media player with isolated-emulator pixel/control evidence;
its controller matrix remains open. AirPlay remains protocol foundations,
not Apple-device interoperability claims.

**实验性预览：Native 播放与真实屏幕采集开发中。** Sender 保留 30 秒测试流，
新增 Android 10+ 系统授权屏幕共享，可选捕获允许共享的播放音频（不采集麦克风）。
真机结果由设备持有人自行安装并反馈，不把模拟器或请求 60 fps 等同真机验收。
操作见[参考测试流](docs/testing/ANDROID_REFERENCE_SENDER.md)与[屏幕共享说明](docs/testing/ANDROID_SCREEN_CAPTURE.md)。

Receiver 新增[实验性 DLNA 播放入口](docs/testing/ANDROID_DLNA_PLAYBACK.md)：显式选择可信
局域网后，可接收控制端自身提供、支持 HTTP Range 的 MP4/MP3/WAV。没有 PIN 认证，
离开页面即停止；第三方媒体主机、HTTPS、图片、字幕和完整控制端兼容矩阵尚未交付。

Sender 支持前台扫描并选择局域网接收器（Experimental，仅本地 IPv4），
也保留手动连接。发现广播未经认证，选择后仍须核对 Receiver 上的完整 TLS
指纹并输入 PIN；不会自动信任或连接。详见[发现边界](docs/adr/0124-android-sender-discovery.md)。

匹配版本还可显式启用[Sender Keystore 身份签名](docs/testing/ANDROID_SENDER_IDENTITY.md)：
Rust 验证 P-256 挑战签名，Receiver 核对完整发送端指纹并批准，或匹配已明确保存的信任记录。
仍需 TLS/PIN，默认关闭，不自动记住、不免 PIN、不在失败后降级；真机身份兼容矩阵尚待验证。

Receiver 的网络变化通知现在保留现有 Rust 连接与配对信息，不再强制重启。
真实 QUIC 的同地址短丢包测试与[设备测试说明](docs/testing/ANDROID_NETWORK_CONTINUITY.md)
已加入；换网换 IP、已失败会话重连及真机短断验收仍未完成。

Receiver 现有应用内音量、静音和遥控器方向键操作：只调节帧舟声音，记住
已完成的调整，调高音量不会自动解除静音。详见[声音测试说明](docs/testing/ANDROID_AUDIO_CONTROLS.md)。
这不是发送端网络遥控；真实播放链路已在隔离模拟器验证，真机听感仍待反馈。

匹配版本支持 Native 屏幕共享横竖屏/窗口尺寸更新：Rust 在同一已认证连接内
协商新视频配置，音频和时间线不重置；Receiver 自身旋转保留播放 Surface 并按比例显示。
旧 Receiver 会在开始媒体前拒绝新模式，请同时更新两端。详见
[旋转验证记录](docs/evidence/2026-10-14-video-reconfiguration/README.md)；仍不等于真机性能或 M2 完成。

按里程碑的证据与未满足退出条件见 [`docs/STATUS.md`](docs/STATUS.md)；所有
M0–M5 能力目前仍按兼容性矩阵标记为 Experimental/Conditional，未把真机或
Apple 设备尚未验证的行为写成完成。

See [PLAN.md](PLAN.md) for the complete product definition, architecture, milestones, quality gates, and release criteria.

Maintainers: [protected dual-APK release signing](docs/ANDROID_RELEASE_SIGNING.md)
documents environment secrets, owner approval, signed artifacts and the debug-to-release migration boundary.

[Third-party notices](THIRD_PARTY_NOTICES.md) describe the verified license bundle
embedded in both distributable APKs and published with them.

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
announcements. Its [output binding](docs/adr/0129-dlna-playback-dispatch.md)
now requires backend acknowledgment before committing playback control state;
unbound output mutations fail instead of simulating success. The opt-in
[Android host](docs/adr/0131-android-dlna-player.md) now binds this boundary to
Rust-owned random-access media reads and a single-Looper OS MediaPlayer.
[GENA eventing](docs/adr/0130-dlna-controller-eventing.md)
now follows advertised event URLs with service-scoped random SIDs, initial SEQ 0,
bounded coalesced updates and AVTransport/RenderingControl LastChange XML.
Callback delivery uses an explicitly plain-HTTP client or caller-injected
HTTP/HTTPS stream boundary. The Android host rate-limits callbacks and accepts
decoder position/end/failure feedback; complete SCPD schemas, images/subtitles,
broader origin approval and named-client interoperability remain open. URL and
callback authorization/address pinning stay in Rust, not the platform player.

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
