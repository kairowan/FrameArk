# FrameArk（帧舟）完整产品与研发计划

> Every stream finds a screen.  
> 让每一道媒体流，都能抵达一块屏幕。

文档状态：产品与工程总纲（Draft）  
项目定位：以 Rust 为核心的跨平台投屏、媒体接收与实时串流生态  
建议仓库：`frameark/frameark`

---

## 1. 项目摘要

FrameArk 不是单一的 Android 投屏 APK，而是一套由 Rust 核心、跨平台接收端、跨平台发送端、自有协议、兼容协议、SDK、后台服务和诊断工具组成的完整生态。

产品最终应具备三项核心能力：

1. **跨平台接收**：在 Android、Windows、Linux、macOS 和树莓派上接收屏幕、音频、图片、文件与媒体 URL。
2. **主流协议兼容**：以明确边界支持 AirPlay/RAOP、DLNA/UPnP，并为 Cast V2 与 Miracast 提供受限兼容能力。
3. **自主可控协议**：通过 FrameArk Native Protocol 为 Android、桌面和浏览器提供不依赖 Apple、Google 认证体系的低延迟投屏能力。

FrameArk 的产品承诺是“开放、跨平台、局域网优先、默认私密、能力可验证”，而不是宣称能够绕过 DRM、设备认证或系统权限限制。

---

## 2. 产品目标与非目标

### 2.1 产品目标

- Rust 持有协议、会话、网络、同步、加密、发现与策略等核心状态。
- Kotlin、Swift 和桌面 UI 只承担系统能力接入、硬件编解码、渲染、音频输出和界面。
- 默认无需账号、无需云服务，局域网内即可完成发现、配对和投屏。
- 同一接收器可以同时发布 FrameArk Native、AirPlay、RAOP 和 DLNA 服务。
- 所有协议能力均可通过自动化测试、兼容矩阵和诊断报告验证。
- 核心能力可通过 Rust API、C ABI、Kotlin API 和命令行工具复用。
- 协议层和平台层解耦，Android App 被移除后 Rust 核心仍可独立运行。

### 2.2 明确非目标

- 不破解或绕过 FairPlay、Widevine、HDCP 等 DRM/内容保护机制。
- 不把 Cast V2 兼容模式宣传成获得 Google 认证的 Chromecast 替代品。
- 不承诺普通 Android APK 能在所有设备上充当 Miracast Sink。
- 不自行设计密码算法；传输安全使用成熟的 TLS 1.3、QUIC 和系统安全存储。
- 不在第一阶段同时实现所有协议、所有平台和所有高级功能。
- 不以复制受限制代码为捷径；协议兼容工作必须保留来源、许可证和实现记录。

---

## 3. 品牌与产品命名

| 用途 | 正式名称 |
|---|---|
| 总项目 | `FrameArk` |
| 中文名 | 帧舟 |
| Rust 核心 | `frameark-core` |
| Android 接收端 | `FrameArk Receiver` |
| 桌面接收端 | `FrameArk Desktop` |
| 投屏发送端 | `FrameArk Sender` |
| 后台服务 | `framearkd` |
| 命令行工具 | `frameark-cli` |
| 协议实验室 | `frameark-lab` |
| Rust SDK | `frameark-sdk` |
| AirPlay 模块 | `frameark-airplay` |
| DLNA 模块 | `frameark-dlna` |
| 自有协议 | `FrameArk Native Protocol`（暂简称 FANP） |

DNS-SD 服务名建议由原设想中的 `_opencast._udp` 改为 `_frameark._udp`。线上的协议标识、包名和商标在首次公开发布前统一冻结。

项目简介：

> A Rust-powered, cross-platform screen mirroring and media receiver.

中文简介：

> 以 Rust 为核心的跨平台投屏与媒体接收框架。

---

## 4. 用户与主要场景

### 4.1 目标用户

- 家庭用户：把手机、电脑中的画面、音乐、照片和视频投到电视。
- 会议室与教室：低延迟演示、多人排队投屏、远程控制和设备策略管理。
- 开发者与设备厂商：通过 SDK、C ABI 或系统集成版嵌入接收能力。
- 开源与协议研究者：使用 CLI、录包回放、兼容性检测和模糊测试工具。
- 树莓派与自建媒体中心用户：运行无界面的 `framearkd`。

### 4.2 核心用户旅程

1. 接收器启动并在局域网发布可用协议和能力。
2. 发送端通过发现、二维码或手动 IP 找到接收器。
3. 用户通过免验证、PIN、电视确认或已信任设备完成授权。
4. 双方协商协议、编解码器、分辨率、帧率、HDR 与音频能力。
5. 接收端建立媒体管线，开始播放并持续反馈网络与解码状态。
6. 方向、分辨率、网络和音轨发生变化时，会话不中断或自动恢复。
7. 会话结束后释放资源、恢复待机界面并生成可选诊断摘要。

---

## 5. 完整产品组成

| 产品 | 主要技术 | 职责 | 首要用户 |
|---|---|---|---|
| `frameark-core` | Rust | 公共类型、会话、事件、策略与跨平台核心 | 全部产品 |
| `framearkd` | Rust | 无界面接收服务、配置和管理 API | Linux/服务器/嵌入式 |
| FrameArk Receiver | Kotlin + Rust | Android TV、盒子、平板、车机接收端 | 终端用户 |
| FrameArk Desktop | Rust + 原生/跨平台 UI | Windows、Linux、macOS、树莓派接收端 | 桌面与会议室 |
| FrameArk Sender for Android | Kotlin + Rust | Android 屏幕、音频和媒体发送 | Android 用户 |
| FrameArk Sender for Desktop | Rust + 平台采集 | 桌面、窗口、区域与系统音频发送 | 桌面用户 |
| FrameArk Web Sender | WebRTC | 浏览器标签页、窗口和桌面分享 | 临时访客 |
| `frameark-sdk` | Rust/C/Kotlin/Swift | 嵌入式接入与二次开发 | 开发者/厂商 |
| `frameark-cli` | Rust | 管理、媒体推送、状态查询 | 管理员 |
| `frameark-lab` | Rust | 抓包、回放、协议调试、兼容性检测 | 开发者 |
| FrameArk Web Admin | Rust Web 服务 + Web UI | 配置、日志、会话、设备策略 | 管理员 |

---

## 6. 支持边界与产品口径

所有能力必须标注成熟度，避免把“实验室可运行”写成“正式兼容”。

| 等级 | 含义 |
|---|---|
| Stable | 已进入兼容矩阵、回归测试和版本兼容承诺，可作为正式功能宣传 |
| Beta | 主要流程可用，但设备覆盖、异常恢复或性能尚未达标 |
| Experimental | 默认关闭，不作兼容承诺，接口可能变化 |
| Unsupported | 由于 DRM、认证、硬件或系统权限，不纳入产品承诺 |

### 6.1 最终目标矩阵

| 能力 | 最终目标 | 边界 |
|---|---|---|
| FrameArk Native 局域网投屏 | Stable | 项目自主维护的主协议 |
| iPhone/Mac 普通屏幕镜像 | Stable 目标 | 不包含 FairPlay 保护内容和 AWDL 普遍支持 |
| AirPlay/RAOP 音频 | Stable 目标 | 多房间能力需单独验证 |
| AirPlay URL/HLS 播放 | Stable 目标 | 受媒体 URL、编码和 DRM 限制 |
| DLNA 媒体推送 | Stable | 以公开 UPnP/DLNA 能力为基础 |
| Android/Windows/Linux/macOS 发送 | Stable | 通过 FrameArk Native Protocol |
| 浏览器投屏 | Stable | 通过 WebRTC Gateway，不等同于浏览器原生 QUIC 客户端 |
| Cast V2 | Experimental/Compat | 不保证官方应用、Google Home 或认证设备行为 |
| Miracast | Experimental/Conditional | 仅在具备 WFD Sink 和所需系统权限的设备上提供 |
| 互联网远程投屏 | Optional | 必须显式启用并部署 STUN/TURN/中继服务 |
| 4K60、HDR、多声道 | Conditional | 取决于发送端、网络、解码器和显示链路 |
| DRM/HDCP 内容 | Unsupported | 不破解、不绕过、不承诺 |

---

## 7. 总体技术架构

### 7.1 分层模型

```text
┌───────────────────────────────────────────────────────────┐
│ Apps: Android Receiver / Desktop / Sender / Web Admin     │
├───────────────────────────────────────────────────────────┤
│ Platform: JNI / C ABI / MediaCodec / VideoToolbox / VAAPI │
├───────────────────────────────────────────────────────────┤
│ Media: Session / Clock / Jitter / Codec / Render Contract │
├───────────────────────────────────────────────────────────┤
│ Protocols: Native / AirPlay / RAOP / DLNA / Cast / WFD    │
├───────────────────────────────────────────────────────────┤
│ Transport: QUIC / TLS / HTTP / RTSP / RTP / SSDP / mDNS   │
├───────────────────────────────────────────────────────────┤
│ Foundation: Identity / Trust / Config / Events / Metrics  │
└───────────────────────────────────────────────────────────┘
```

依赖方向只能自上而下；协议模块不得直接操作 Android `Surface`、音频设备或 UI。平台层通过稳定接口向媒体层提供解码、渲染、音频和系统生命周期能力。

### 7.2 控制面与媒体面

- **控制面**：发现、配对、鉴权、能力协商、会话状态、播放控制和统计事件。
- **媒体面**：视频、音频、字幕、文件和元数据数据流。
- 控制事件可通过普通 FFI 消息和回调传递。
- 高频媒体数据应使用引用计数缓冲、环形缓冲或直接缓冲区，避免跨 FFI 反复复制。
- 每一条媒体轨道都包含独立的时间基、序号、编解码参数和生命周期。

### 7.3 核心会话状态机

```text
Idle → Discovered → Connecting → Authenticating → Negotiating
     → Preparing → Streaming ↔ Reconfiguring → Draining → Closed
                         ↓
                     Recovering
```

任何协议适配器都必须映射到统一会话状态机。错误必须区分可恢复错误、协议错误、授权错误、媒体能力错误和平台错误。

---

## 8. 建议的单仓库结构

```text
FrameArk/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ LICENSES/
├─ docs/
│  ├─ architecture/
│  ├─ protocols/
│  ├─ compatibility/
│  ├─ security/
│  └─ adr/
├─ specs/
│  └─ fanp/
├─ crates/
│  ├─ frameark-core/
│  ├─ frameark-discovery/
│  ├─ frameark-transport/
│  ├─ frameark-media/
│  ├─ frameark-native/
│  ├─ frameark-airplay/
│  ├─ frameark-dlna/
│  ├─ frameark-cast-compat/
│  ├─ frameark-miracast/
│  ├─ frameark-trust/
│  ├─ frameark-diagnostics/
│  ├─ frameark-api/
│  └─ frameark-ffi/
├─ services/
│  ├─ framearkd/
│  └─ frameark-web-gateway/
├─ apps/
│  ├─ android/receiver/
│  ├─ android/sender/
│  ├─ desktop/receiver/
│  ├─ desktop/sender/
│  └─ web/sender/
├─ tools/
│  ├─ frameark-cli/
│  ├─ frameark-lab/
│  ├─ packet-replay/
│  └─ protocol-fuzz/
├─ fixtures/
│  ├─ packets/
│  ├─ media/
│  └─ interoperability/
└─ .github/workflows/
```

初期不要立刻拆成三十多个 crate。先按上面的十余个边界建立模块；当编译隔离、复用、所有权或发布确有需要时，再把 `rtsp`、`rtp`、`plist`、`soap`、`gena`、`jitter-buffer` 等拆成独立 crate。

---

## 9. Rust 核心模块计划

### 9.1 Foundation

- `frameark-core`：设备、会话、轨道、能力、错误、事件总线和生命周期。
- `frameark-trust`：设备身份、信任记录、PIN/一次性令牌、黑名单和平台密钥库抽象。
- 配置系统：分层配置、默认值、迁移、校验、热更新和敏感字段隔离。
- 统一错误码：稳定错误类别与可本地化用户提示分离。

### 9.2 Discovery 与 Transport

- mDNS/DNS-SD：多网卡、IPv4/IPv6、缓存、冲突处理、接口变化。
- SSDP：`M-SEARCH`、`NOTIFY`、alive/byebye 和租约。
- HTTP/HTTPS、HTTP Reverse、RTSP、RTP/RTCP、QUIC/TLS 1.3。
- 网络策略：局域网判断、接口绑定、端口策略、速率限制和连接超时。

### 9.3 Media

- 统一媒体会话、视频/音频/字幕/元数据轨道模型。
- H.264/H.265 NAL、关键帧、参数集和分片重组。
- AAC、ALAC、PCM、Opus 能力模型；解码由平台或可选后端提供。
- 自适应抖动缓冲、时钟偏差估计、A/V 同步和丢帧策略。
- 码率、RTT、丢包、解码耗时、缓冲深度等反馈模型。
- 硬件解码优先，软件解码作为可选构建或运行时回退。

### 9.4 API 与 FFI

- Rust API 使用语义化版本并区分稳定 API 与内部 API。
- C ABI 只暴露不透明句柄、固定宽度类型和版本化结构体。
- Android JNI 负责生命周期和系统能力桥接，不在 Kotlin 侧复制协议状态机。
- 高频帧数据使用 `DirectByteBuffer`、共享句柄或平台表面，控制消息使用序列化事件。
- 明确线程归属、回调线程、释放顺序和崩溃边界。

---

## 10. FrameArk Native Protocol（FANP）计划

FANP 是项目长期最重要的协议资产。规范应先于多平台客户端扩张，并公开版本、状态机和兼容规则。

### 10.1 传输与安全

- 局域网发现：mDNS + DNS-SD，建议服务 `_frameark._udp`。
- 传输：QUIC + TLS 1.3；控制和文件使用可靠流，实时音视频优先使用 Datagram。
- 身份：持久设备身份与公钥指纹；私钥进入 Android Keystore、Keychain 或系统安全存储。
- 配对：电视确认、PIN、二维码一次性令牌、已信任设备快速重连。
- 默认只监听局域网接口；跨互联网必须显式启用并采用独立威胁模型。

### 10.2 协议消息

最小握手流程：

1. `ClientHello`：协议版本、客户端身份和支持能力。
2. `ServerHello`：接收器身份、挑战、支持能力和授权模式。
3. `PairRequest/PairResult`：PIN、电视确认或已信任设备验证。
4. `SessionOffer/SessionAnswer`：轨道、编码、分辨率、帧率、HDR 和延迟目标。
5. `Start/Update/Stop`：启动、动态重配置和关闭会话。
6. `Stats/Feedback`：RTT、丢包、抖动、解码队列、NACK 和关键帧请求。

### 10.3 媒体与恢复

- 视频默认 H.264，音频默认 Opus；AAC 用于更广泛的硬件解码兼容。
- H.265、AV1、HDR、多声道作为能力协商项，不作为连接前提。
- 每包包含会话、轨道、序号、时间戳、帧边界和依赖信息。
- 参数集、关键配置和控制状态必须可可靠重发。
- 支持 NACK、PLI/IDR 请求、可选 FEC 和自适应缓冲。
- Connection ID 与会话票据用于短暂 Wi-Fi 切换后的安全恢复。
- Major 版本不兼容，Minor 版本通过 Capability 位和忽略未知字段向前兼容。

### 10.4 FANP 规范交付物

- 协议术语、威胁模型和状态机。
- 消息 schema 与兼容规则。
- 传输通道、帧格式和时间戳规范。
- 配对、信任撤销和密钥轮换规范。
- 参考发送端、参考接收端和一致性测试套件。
- Wireshark dissector 或等价的可读调试工具。

---

## 11. 协议实施计划

### 11.1 DLNA/UPnP

交付范围：

- SSDP、设备/服务描述、AVTransport、RenderingControl、ConnectionManager。
- SOAP、GENA、DIDL-Lite、HTTP Range 和 DLNA ContentFeatures。
- 图片、音乐、视频、封面、字幕、进度、Seek 和音量控制。
- 媒体 URL 安全策略、格式探测和可选 FFmpeg/GStreamer 转码接口。

完成标准：通过固定客户端矩阵完成发现、加载、播放、暂停、Seek、停止、事件订阅和断线恢复；未知 XML 字段不得导致崩溃。

### 11.2 AirPlay/RAOP

按风险递增顺序实施：

1. mDNS 服务发布、RTSP、HTTP、XML/Binary Plist 基础。
2. RAOP 音频、RTP/RTCP、PCM/AAC/ALAC、元数据和封面。
3. 配对、验证、设备身份与加密会话。
4. AirPlay URL/HLS 播放和远程控制。
5. H.264 屏幕镜像、方向变化、音频和 A/V 同步。
6. H.265、AirPlay 2 音频与多房间同步按兼容矩阵逐项推进。

完成标准：每个宣称支持的 Apple OS/设备组合都进入回归矩阵；不得把实验设备成功等同于完整协议支持。FairPlay 内容与 AWDL 普遍兼容不进入承诺。

### 11.3 Cast V2 兼容模式

- TLS、Protobuf、Connection、Heartbeat、Receiver 和 Media Namespace。
- 自定义 FrameArk Cast App 与开源控制端优先。
- 官方 YouTube、Google Home、认证设备和 DRM 应用不作保证。
- 默认标记为 Experimental，只有明确测试过的发送端进入兼容列表。

### 11.4 Miracast/Wi-Fi Display

- Rust 实现 WFD RTSP/RTP 状态机、H.264 和音频接收。
- Wi-Fi Direct、WFD IE、UIBC 和系统 Sink 能力放入平台适配层。
- 普通 APK 不尝试绕过系统签名或厂商驱动限制。
- 仅为系统集成版、Root 插件或确认具备 WFD Sink 能力的设备启用。

---

## 12. Android Receiver 计划

### 12.1 Kotlin 职责

- Activity、前台 Service、Compose/Compose TV 界面和遥控器焦点。
- `Surface`/`TextureView`、MediaCodec、AudioTrack 和音频路由。
- 网络权限、MulticastLock、Wi-Fi 状态、开机启动和通知。
- Android Keystore、系统语言、无障碍、画中画和电源管理。

### 12.2 Rust 职责

- 所有发现和投屏协议、会话状态、配对与信任策略。
- RTP/RTCP、QUIC、同步、缓冲、媒体轨道和诊断数据。
- 与 Android 无关的配置、设备策略和兼容性规则。

### 12.3 界面

- 待机页：设备名称、网络状态、二维码、PIN 和协议状态。
- 连接确认页：发送端、协议、请求能力、允许/拒绝/始终允许。
- 播放页：标题、进度、音量、字幕、音轨和可选 Debug Overlay。
- 设置页：名称、房间、授权、显示、音频、网络、协议、隐私和诊断。
- 历史/信任页：已信任设备、黑名单、撤销权限和私密模式。

### 12.4 初始设备基线

- 第一参考平台建议为 Android TV/盒子 ARM64、2 GB 内存、具备 H.264 1080p60 硬件解码。
- `minSdk` 在原型完成后依据 MediaCodec、音频捕获和设备覆盖测试冻结，避免仅凭设想承诺。
- ARMv7 可作为兼容构建，x86_64 用于模拟器和测试；正式性能结论以真机为准。

---

## 13. 发送端计划

### 13.1 平台采集

| 平台 | 视频采集 | 音频采集 | 首要输出 |
|---|---|---|---|
| Android | MediaProjection | AudioPlaybackCapture/麦克风 | FANP |
| Windows | Windows Graphics Capture/DXGI | WASAPI Loopback | FANP |
| Linux | PipeWire/XDG Desktop Portal | PipeWire/PulseAudio | FANP |
| macOS | ScreenCaptureKit | CoreAudio | FANP |
| Browser | `getDisplayMedia` | Web Audio | WebRTC Gateway |
| CLI | 文件、URL、标准输入 | 文件音频 | FANP/DLNA 控制 |

### 13.2 共同功能

- 整屏、显示器、窗口、应用或区域捕获。
- 系统声音、麦克风或混音；明确显示采集状态。
- 自动/流畅/高清/演示预设，以及分辨率、帧率和码率上限。
- 软件编码与 GPU 硬件编码；能力不足时可预测地降级。
- 暂停、黑屏、冻结最后一帧、鼠标高亮、点击动画和激光笔。
- 已授权远程输入、剪贴板和文件投送；默认关闭敏感权限。
- 设备收藏、自动重连、多接收器和脱敏诊断报告。

---

## 14. 接收端完整功能域

### 14.1 设备与连接

- 自定义设备名称、房间、图标与永久身份。
- 无验证、PIN、电视确认、信任设备和黑名单。
- 多协议同时发现、手动 IP、二维码、IPv4/IPv6 和多网卡。
- 单视频会话默认独占；音频支持拒绝、抢占、暂停或混音策略。
- 当前占用时提供请求队列和明确的发送端反馈。

### 14.2 显示与媒体

- 保持宽高比、填充裁剪、原始比例、旋转、镜像和过扫描校正。
- 黑色、主题或模糊背景；按能力协商 HDR/SDR。
- 硬件解码、软件回退、动态分辨率/帧率和落后丢帧。
- 音量、音频输出、延迟校准、夜间模式和条件多声道。
- 播放、暂停、Seek、进度、字幕、音轨、封面和元数据。

### 14.3 系统与管理

- 开机启动、后台接收、自动唤醒、屏保抑制、断线恢复和结束回主页。
- 可选画中画、Web 管理、REST/WebSocket API、CLI 和配置文件。
- 设备、协议、房间、时间段和用户授权策略。
- 中文、英文起步；支持大字体、TalkBack 和遥控器完整操作。

---

## 15. 安全与隐私计划

### 15.1 威胁模型

至少覆盖：局域网陌生设备、重放攻击、中间人、暴力 PIN、畸形协议包、XML/Plist 资源耗尽、恶意媒体 URL、管理接口越权、日志泄密和远程输入滥用。

### 15.2 强制安全基线

- 默认仅局域网监听；管理 API 默认需要本机或已认证访问。
- PIN 限速、指数退避、会话超时和连接上限。
- 所有解析器限制消息大小、嵌套深度、集合长度和内存预算。
- XML 禁用外部实体；媒体 URL 防止 SSRF、回环/内网越权和危险重定向。
- 持久私钥不得明文写入普通配置文件。
- 远程输入、剪贴板、文件接收和互联网访问必须单独授权。
- 日志默认脱敏，不记录 PIN、令牌、私钥、完整媒体内容和敏感 URL 参数。
- 依赖审计、SBOM、许可证扫描、漏洞响应和签名发布进入 CI/CD。

### 15.3 隐私原则

- 默认无账号、无云依赖、无遥测。
- 兼容性数据和崩溃报告只能由用户主动选择上传。
- 私密模式不保存连接历史、设备名称和媒体元数据。
- 诊断包在导出前显示内容范围，并提供自动脱敏结果。

---

## 16. 诊断、观测与协议实验室

- 网络：接口、地址、组播、端口、RTT、丢包、抖动、重传和估算带宽。
- 会话：发送端、协议、状态、连接时长、授权模式和恢复次数。
- 视频：编码、Profile、分辨率、帧率、码率、解码耗时、错误帧和丢帧。
- 音频：编码、采样率、声道、缓冲、输出设备和 A/V 偏差。
- 系统：CPU、内存、温度、缓冲区与硬件解码器。
- 发现诊断：mDNS、DNS-SD、SSDP 发布与请求统计。
- 报告：脱敏 JSON/ZIP、可选 PCAP、Packet Replay 和兼容性报告。
- 展示：本地 Debug Overlay、Web 实时日志和 CLI 状态。
- 部署：可选 Prometheus Metrics 与 OpenTelemetry；默认关闭外发。

`frameark-lab` 应支持构造握手、检查服务记录、回放脱敏会话、验证协议状态机和比较两次兼容测试结果。

---

## 17. 测试与质量计划

### 17.1 自动化测试

- 单元测试：状态机、RTSP、RTP、Plist、SOAP、CBOR/Protobuf、时间戳和配置迁移。
- 属性测试：任意输入不得引发 panic、无限循环或无界分配。
- Fuzz：网络包、XML、Plist、NAL、字幕、图像元数据和协议消息。
- Golden/Fixture：合法、边界、损坏和历史兼容数据。
- 集成测试：发现 → 配对 → 协商 → 播放 → 重配置 → 恢复 → 结束。
- ABI 测试：C/JNI 结构布局、生命周期、错误传播和跨版本兼容。
- 升级测试：新旧发送端、接收端、配置和信任数据库迁移。

### 17.2 设备与网络矩阵

- iPhone、iPad、Mac；主流 Android 手机与 Android TV/盒子。
- Windows、Linux X11/Wayland、macOS 和树莓派。
- 普通路由器、Mesh、访客网络、AP 隔离、多网卡、IPv6 和网络切换。
- H.264/H.265、AAC/ALAC/PCM/Opus 以及不同分辨率、帧率和声道。
- 1～2 GB 低端设备与主流 4K 设备。

### 17.3 稳定性场景

- 1、8、24 小时连续播放。
- 重复连接/断开、旋转、切分辨率、切音轨和后台/前台切换。
- 弱网：延迟、抖动、丢包、乱序、限速、短暂断网和地址变化。
- 内存泄漏、句柄泄漏、线程泄漏、CPU 峰值和热降频。
- 服务异常后的资源清理、自动恢复和最后状态一致性。

### 17.4 建议质量门槛

以下是参考硬件上的目标，不是对所有设备的无条件承诺：

- Native 1080p60 局域网投屏端到端延迟：Beta `<250 ms`，Stable 目标 `<150 ms`。
- 长时间 A/V 同步误差：Stable 目标控制在 `±40 ms`。
- 常规局域网首次连接：目标 `<3 s`，不含用户确认时间。
- 24 小时稳定性测试：无崩溃、无持续内存增长、会话可正常结束。
- 1% 随机丢包下可继续观看，并能在关键帧请求后恢复画面。
- Stable 协议解析器必须进入持续 fuzz，并对已发现崩溃保留回归样本。

---

## 18. CI/CD 与发布工程

### 18.1 持续集成

每个合并请求至少执行：

- Rust 格式、静态检查、单元/集成测试、文档测试和最小支持版本检查。
- Android Lint、Kotlin 测试、JNI/NDK 构建和模拟器冒烟测试。
- Linux、Windows、macOS 的核心编译与测试。
- 依赖漏洞、许可证、秘密、SBOM 和供应链检查。
- 受影响协议的 fixture 回放和兼容回归。

夜间任务执行设备测试、长时间测试、fuzz、弱网测试和性能基准。高成本真机任务不阻塞普通文档提交，但必须阻塞 Stable 发布。

### 18.2 发布通道

- `nightly`：自动构建，仅供开发与问题复现。
- `alpha`：功能尚不完整，允许数据结构和协议变更。
- `beta`：功能冻结，集中完成兼容性、性能和迁移验证。
- `stable`：签名产物、升级说明、SBOM、校验值、兼容矩阵和已知限制齐全。

### 18.3 版本策略

- 产品、SDK、FANP 规范分别版本化，但在发布清单中记录兼容关系。
- Rust crate 遵循 SemVer；不稳定内部 crate 可暂不独立发布。
- FANP Major 版本变更必须提供迁移文档和至少一个版本周期的双栈计划。

---

## 19. 法律、许可证与合规

- 建议自研核心采用 `Apache-2.0 OR MIT` 双许可证；正式决定写入治理文档。
- 引入第三方代码前核对许可证兼容性，保留 SPDX 标识和 `THIRD_PARTY_NOTICES`。
- 不能直接复制 GPL 项目代码进入宽松许可证核心；参考行为与复制实现必须严格区分。
- FFmpeg/GStreamer 的构建选项、动态链接方式和编解码器许可证单独评审。
- H.264/H.265/AAC 等可能涉及地区性专利或分发义务，二进制发行前进行法律审查。
- AirPlay、Chromecast、DLNA 等名称仅用于互操作描述，不暗示官方认证。
- 商业化、应用商店上架和域名购买前，对 FrameArk/帧舟进行正式商标检索。

---

## 20. 研发路线图

路线按“依赖和验收门”推进，不按功能数量推进。下列工期是假设 4～6 名熟悉 Rust、Android 和媒体系统的全职工程师后的粗估；单人开发通常需要数年，不能直接套用团队工期。

### M0：工程与规范基础（4～6 周）

交付物：

- 单仓库、Rust workspace、Android 壳工程、CI、许可证和贡献指南。
- 架构决策记录、错误模型、日志规范、测试 fixture 规则。
- `frameark-core` 会话骨架、配置、事件和平台接口。
- 品牌包名、crate 名、DNS-SD 名称和 FANP 工作名称冻结。

退出条件：三平台核心库可编译；Android 能加载 Rust 库；CI 能运行最小测试；没有未记录的关键架构分歧。

### M1：Native 最小纵向闭环（8～12 周）

交付物：

- mDNS 发现、QUIC/TLS、临时配对、能力协商和单会话状态机。
- 测试发送器生成 H.264 + Opus/AAC 流。
- Android Receiver 完成硬件解码、渲染、音频和基础同步。
- CLI 可发现、连接、查询状态和结束会话。

退出条件：参考局域网中可重复完成“发现 → 授权 → 1080p30 播放 → 结束”；断开后资源完全释放；诊断能定位主要阶段。

### M2：Android Receiver MVP（8～10 周）

交付物：

- 待机、授权、播放、设置、信任设备和诊断界面。
- PIN/电视确认/信任设备、Keystore、前台服务和开机启动。
- 动态旋转、分辨率变化、断线恢复、音量和基础遥控。
- Android Sender MVP 或可用于真实屏幕采集的桌面参考发送端。

退出条件：1080p60 H.264 在参考设备上达到 Beta 指标；一小时稳定运行；网络短断可恢复；普通用户无需开发工具即可完成投屏。

### M3：DLNA 与媒体推送（6～10 周）

交付物：

- 完整 MediaRenderer 基础服务、事件订阅、元数据和 HTTP Range。
- 图片、音乐、视频、字幕、Seek、音量和媒体控制。
- URL 安全策略和格式能力报告。

退出条件：选定的 Android/iOS/桌面控制端全部通过核心用例；畸形 XML 和超大描述数据通过安全测试；两小时媒体播放与 Seek 稳定。

### M4：RAOP 与 AirPlay 媒体（10～16 周）

交付物：

- RTSP/RTP、Plist、配对/验证、RAOP 音频、元数据和远程控制。
- AirPlay URL/HLS 播放。
- 设备兼容矩阵和脱敏会话回放样本。

退出条件：目标 Apple 设备/系统组合在音频和 URL 播放核心场景通过回归；长时间同步、重连和安全测试达标。

### M5：AirPlay 屏幕镜像（12～24 周，高风险）

交付物：

- H.264 镜像数据处理、镜像音频、方向变化和 A/V 同步。
- H.265 作为条件能力；异常参数集、丢包和重配置恢复。
- Android 与 Desktop Receiver 共用协议实现。

退出条件：兼容矩阵中规定的 iPhone、iPad、Mac 普通镜像场景达到 Beta；不支持的 DRM/AWDL 情况有明确提示而非静默失败。

### M6：桌面接收端与正式发送端（12～18 周）

交付物：

- `framearkd`、Windows/Linux/macOS 接收和基础 UI。
- Android、Windows、Linux、macOS Sender 的捕获、音频、硬件编码和统计。
- 文件/URL 发送、远程输入、剪贴板与多显示器。

退出条件：各平台完成安装、升级、签名和基本无障碍验证；Native 跨平台互操作矩阵通过。

### M7：高级 Native 能力与 Web（10～16 周）

交付物：

- 多接收器、多房间音频、文件通道、中继和动态拥塞控制增强。
- WebRTC Gateway、浏览器 Sender、可选 ICE/STUN/TURN。
- Web Admin、REST/WebSocket、Prometheus/OpenTelemetry 可选集成。

退出条件：本地模式不依赖云；互联网模式有独立安全审核；多设备时钟与恢复指标达到规定阈值。

### M8：兼容实验与 1.0 加固（8～16 周）

交付物：

- Cast V2 兼容模式；Miracast 系统集成实验模块。
- 全量 fuzz、24 小时 soak、低端设备优化、配置迁移和崩溃恢复。
- SDK 文档、示例、兼容数据库、安装包、SBOM 和安全响应流程。

退出条件：所有 Stable 功能拥有明确兼容矩阵、自动回归、已知限制和升级方案；Experimental 功能默认隔离；阻断级缺陷归零。

---

## 21. 优先级与首个可发布版本

### 21.1 P0：必须先完成

- Rust 核心边界、统一会话状态机和媒体管线。
- Android Receiver 的 H.264/AAC 或 Opus 纵向闭环。
- FANP 发现、配对、QUIC/TLS、能力协商和恢复基础。
- 硬件解码、渲染、音频、日志、诊断和最小安全基线。

### 21.2 P1：形成有用产品

- DLNA 媒体推送。
- Android/桌面真实发送端。
- RAOP 音频与 AirPlay URL 播放。
- 信任设备、设置、CLI、Web Admin 基础。

### 21.3 P2：建立差异化

- AirPlay 屏幕镜像。
- 完整桌面接收端、Web Sender、远程控制、多接收器。
- 网络自适应、兼容数据库和协议实验室。

### 21.4 P3：受限或实验能力

- Cast V2 兼容模式。
- Miracast 系统适配。
- AirPlay 2 多房间、互联网中继、4K60、HDR 和 AV1。

首个公开 MVP 不应包含“全协议接收器”的宣传。建议口径为：

> FrameArk Receiver Preview：通过 FrameArk Native Protocol 在局域网接收 Android/桌面画面，并提供基础 DLNA 媒体播放。

---

## 22. 团队与责任划分

理想的最小全职团队：

| 方向 | 建议人数 | 责任 |
|---|---:|---|
| Rust 网络/协议 | 2 | FANP、AirPlay、DLNA、发现、配对、安全 |
| 媒体与性能 | 1 | RTP、同步、抖动、编解码适配、弱网 |
| Android | 1 | Service、MediaCodec、AudioTrack、Compose TV、设备兼容 |
| 桌面/Web | 1 | 捕获、编码、桌面接收器、WebRTC Gateway |
| QA/发布 | 1 | 设备矩阵、自动化、弱网、长稳、打包与发布 |

早期人员不足时，可由同一人兼任，但责任域仍要在 issue、评审和文档中明确。AirPlay 与安全相关改动至少需要第二人评审。

---

## 23. 项目管理方式

### 23.1 工作单元

- Epic：对应一个用户能力或协议里程碑。
- Feature：能够独立演示和验收的纵向功能。
- Task：不超过数日、结果可测试的工程任务。
- ADR：影响模块边界、协议、持久化、FFI 或安全的决策。

### 23.2 每个 Feature 的完成定义

- 代码、测试、文档和诊断事件同时完成。
- 对外行为、错误、超时、取消和资源释放已定义。
- 不支持场景会向用户给出可理解提示。
- 安全与隐私影响已评审；敏感日志已脱敏。
- 至少有一个自动化集成用例和一个真实设备验证记录。
- 若改变公开 API、协议或配置，包含兼容说明与迁移路径。

### 23.3 迭代节奏

- 两周一个开发迭代，每个迭代必须保留可运行主干。
- 每四到六周产出一次可安装的内部版本。
- 每个里程碑先冻结范围，再进入兼容性和稳定性冲刺。
- 任何新协议在成为默认功能前，必须先通过安全、诊断和关闭/恢复测试。

---

## 24. 关键风险与应对

| 风险 | 影响 | 应对策略 |
|---|---|---|
| AirPlay 行为变化或设备差异 | 高 | 分阶段交付、保留录包回放、维护设备/OS 兼容矩阵 |
| Cast 设备认证限制 | 高 | 明确标为兼容模式，优先自定义 App 与开源控制端 |
| Miracast 依赖驱动和系统权限 | 高 | 平台适配独立模块，只对具备能力设备开放 |
| Android 硬件解码器碎片化 | 高 | 设备数据库、能力探测、黑名单、软件回退与参考设备 |
| 音画同步和弱网恢复 | 高 | 统一时钟模型、可观测缓冲、网络仿真和长稳测试 |
| Rust/Kotlin FFI 生命周期错误 | 高 | 句柄模型、线程约束、ABI 测试、集中释放路径 |
| 需求范围过大 | 高 | 只按里程碑退出条件扩张，P3 不阻塞核心发布 |
| 协议/编解码许可证风险 | 高 | 来源记录、法律审查、构建特性隔离、第三方清单 |
| 低端设备性能不足 | 中高 | 零/少复制、硬件优先、分档能力、持续基准 |
| 缺少真机测试 | 中高 | 建立最小设备池、社区兼容报告与可复现诊断包 |

---

## 25. 成功指标

### 25.1 产品指标

- 首次连接成功率、重连成功率、会话正常结束率。
- 各协议/设备组合通过率和回归数量。
- 用户完成“发现到首帧”的时间。
- 投屏期间崩溃、卡死、无声和严重不同步比例。
- 脱敏诊断包能够直接定位问题类别的比例。

### 25.2 工程指标

- Stable 解析器 fuzz 时长与无崩溃周期。
- 主干构建成功率、回归测试时长和 flaky test 比例。
- 参考设备上的延迟、CPU、内存、温度和丢帧趋势。
- 公共 API/协议的破坏性变更次数与迁移覆盖率。
- 已知设备兼容问题拥有自动回归样本的比例。

不应以“支持协议数量”作为唯一成功指标；稳定的一条端到端链路比六个只能演示的协议更有价值。

---

## 26. 1.0 完整验收标准

FrameArk 1.0 可以发布的最低条件：

1. `frameark-core`、`framearkd`、Android Receiver、至少两个正式 Sender 可稳定使用。
2. FANP 规范、参考实现、兼容测试和版本协商完整。
3. DLNA 达到 Stable；AirPlay/RAOP 已明确区分 Stable、Beta 和不支持能力。
4. 所有 Stable 功能覆盖发现、授权、播放、控制、重配置、弱网恢复和正常关闭。
5. Android、Windows、Linux、macOS 的支持范围、安装方式和已知限制可查。
6. 参考设备通过 24 小时稳定性、弱网、安全、升级和资源泄漏测试。
7. 安全模型、隐私说明、依赖审计、SBOM、签名与漏洞响应流程齐全。
8. Cast、Miracast、DRM、AWDL、4K/HDR 的边界在 UI 和文档中一致。
9. SDK、CLI、示例、配置、管理 API 和诊断包格式有版本化文档。
10. 新用户仅依据文档即可完成构建、安装、发现、配对和一次投屏。

---

## 27. 立即执行的第一批任务

1. 确认许可证、包名、crate 前缀、Android application ID 和 FANP 协议简称。
2. 初始化 Rust workspace、Android Receiver、CI 和 ADR 目录。
3. 写出统一 `Device`、`Capability`、`Session`、`Track`、`Event`、`Error` 模型。
4. 定义平台媒体接口：视频配置/帧、音频配置/帧、时钟、渲染状态和背压。
5. 完成 mDNS 多网卡发现原型及自动化网络测试。
6. 完成 QUIC/TLS 的最小安全连接和临时配对，不先加入完整媒体。
7. 用测试图与测试音构建第一个 H.264 + 音频端到端播放闭环。
8. 接入 MediaCodec、AudioTrack 和基础同步，记录首帧/延迟/丢帧指标。
9. 建立脱敏 fixture、Packet Replay 和 parser fuzz 的目录与规则。
10. 在首个真实发送端开始前评审 M1 的数据模型、FFI 和威胁模型。

这十项完成后，项目才进入“扩展真实发送端和兼容协议”的阶段。

---

## 28. 最终产品定义

FrameArk 的完整形态是：

> 一个由 Rust 实现的跨平台媒体接收与实时投屏引擎，正式建设 AirPlay/RAOP、DLNA 和 FrameArk Native Protocol；为 Cast V2 提供受限兼容模式，为具备系统能力的设备提供 Miracast 适配；拥有 Android、桌面和浏览器发送端与接收端，并提供 SDK、后台服务、CLI、协议实验室、诊断工具和 Web 管理后台。

长期护城河不是协议名称的数量，而是以下三项能够共同演进：

1. 可复用、可测试、跨平台的 Rust 媒体与会话核心。
2. 对 AirPlay/RAOP 与 DLNA 的稳定互操作能力。
3. 完全自主、公开、可扩展的 FrameArk Native Protocol。

FrameArk 应从一条稳定的 Native 纵向链路出发，逐步长成完整生态，而不是从一张“全协议清单”同时开工。
