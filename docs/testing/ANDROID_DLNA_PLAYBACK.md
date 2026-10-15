# Android DLNA 播放预览 / Experimental player

本入口是 M3 的一个真实播放增量，不是完整 DLNA 兼容或 M0–M5 验收。
设备持有人自行安装 APK 和操作设备；开发代理不连接你的个人设备。

## 操作与边界

1. 安装匹配版本 Receiver，在主页先停止 Native 接收器。
2. 打开「实验性 DLNA 接收器」，选择「启用 DLNA」、可信局域网 IPv4 地址，
   阅读并确认风险。没有 PIN，网络内其他可达控制端也能控制播放。
3. 使用能在本机提供媒体 HTTP Range 服务的控制端，寻找
   `FrameArk Receiver (Experimental DLNA)`。发送它自身托管的 MP4/MP3/WAV。
4. 测试播放、暂停、拖动进度、音量、停止和再次播放；检查声画、比例和结束状态。
5. 「诊断信息」提供不含媒体 URL/内容的计数。离开页面或点停止会关闭本接收器。

当前只接收私有 IPv4 控制端；媒体主机必须是该控制端的连接地址，且支持确切的
`206 Content-Range`、已知长度（最多 64 GiB）。HTTP 不加密。不能播放由另一个
NAS/公网服务器托管的 URL，也不跟随重定向。Android HTTPS、图片、字幕和播放列表
尚未接入。MP3 是允许尝试的 MIME 类型，尚无真实 MP3 解码证据；具体容器内编解码器
是否可用由系统决定。失败会清空本次媒体状态，需要停止并重新启用以恢复输出后端。
SCPD 已补齐单曲 v1 服务的完整参数/状态变量，但控制端矩阵仍待实测。
支持 FactoryDefaults 恢复音量 50、TRACK_NR=1 回到起点；Next/Previous 在单曲边界报错。
替换正在播放的资源会自动播放新资源。所有必需参数须显式传入，包括空元数据元素。
找不到设备或控制失败请如实反馈，不算测试通过。详见[服务契约](../adr/0132-dlna-service-contracts.md)。

多网卡、其他应用占用 UDP 1900、访客网络/AP 隔离、VPN 和路由器禁止组播都可能导致
发现失败。此版本不会自动修改防火墙、后台重启或切换到其他接口。

请反馈：Receiver/控制端版本、设备型号和系统、媒体容器/编码、是否同网段、各控制动作
是否成功、播放时长、音画/旋转问题，以及不含私密数据的诊断。不要上传生产签名密钥、
完整私人媒体 URL、媒体内容或含个人信息的抓包。两小时稳定性及指定控制端矩阵仍待实测。

## Developer regression (isolated emulator only)

Build matching x86_64 Rust JNI with `--features android-playback-probe` and place it
in `build/probe-jni/x86_64`; build Receiver app/test APKs using
`-PframearkPlaybackProbe=true`. Install only on an agent-owned isolated emulator,
then run `dev.frameark.receiver.DlnaPlaybackTest` with InstrumentationTestRunner.
Always specify the exact authorized serial. These are not distribution APKs.

The test creates original synthetic H.264 MP4/PCM WAV at runtime; no external
media, copied fixture or third-party decoder is included. It asserts real TextureView
pixels, control responses, actual player progress, source replacement/EOS and source
adapter bounds, FactoryDefaults output volume, TRACK_NR and replacement auto-resume.
A separate case starts the normal private-IPv4 foreground service
and verifies page-close teardown. Loopback bypass exists only in probe-feature native
builds; normal APKs cannot start it. See [evidence](../evidence/2026-10-15-dlna-playback/README.md).
