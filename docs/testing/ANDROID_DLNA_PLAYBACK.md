# Android DLNA 播放预览 / Experimental player

本入口是 M3 的一个真实播放增量，不是完整 DLNA 兼容或 M0–M5 验收。
设备持有人自行安装 APK 和操作设备；开发代理不连接你的个人设备。

## 操作与边界

1. 安装匹配版本 Receiver，在主页先停止 Native 接收器。
2. 打开「实验性 DLNA 接收器」，选择「启用 DLNA」、可信局域网 IPv4 地址，
   阅读并确认风险。没有 PIN，网络内其他可达控制端也能控制播放。
3. 使用能在本机提供媒体 HTTP Range 服务的控制端，寻找
   `FrameArk Receiver (Experimental DLNA)`。发送它自身托管的 MP4/MP3/WAV 或 JPEG/PNG。
4. 测试播放、暂停、拖动进度、音量、停止和再次播放；检查声画、比例和结束状态。
5. 「诊断信息」提供不含媒体 URL/内容的计数。离开页面或点停止会关闭本接收器。

当前只接收私有 IPv4 控制端；媒体主机必须是该控制端的连接地址，且支持确切的
`206 Content-Range`、已知长度（最多 64 GiB）。HTTP 不加密。不能播放由另一个
NAS/公网服务器托管的 URL，也不跟随重定向。Android HTTPS、字幕和播放列表
尚未接入。MP3 是允许尝试的 MIME 类型，尚无真实 MP3 解码证据；具体容器内编解码器
是否可用由系统决定。失败会清空本次媒体状态，需要停止并重新启用以恢复输出后端。
SCPD 已补齐单曲 v1 服务的完整参数/状态变量，但控制端矩阵仍待实测。
支持 FactoryDefaults 恢复音量 50、TRACK_NR=1 回到起点；Next/Previous 在单曲边界报错。
替换正在播放的资源会自动播放新资源。所有必需参数须显式传入，包括空元数据元素。
找不到设备或控制失败请如实反馈，不算测试通过。详见[服务契约](../adr/0132-dlna-service-contracts.md)。

### 图片测试

JPEG/PNG 最多 16 MiB；原始边长最多 32768、最多 1 亿像素。系统解码前按 2 的幂降采样，
显示图最多 4096 单边、4,194,304 像素，以 sRGB 按比例居中显示。不是原图质量/广色域承诺。
JPEG 支持 EXIF 1–8 方向；PNG 不读取 EXIF，动画只呈现系统解码的静态帧，没有幻灯片计时。
先加载时为停止/黑屏，Play 显示、Pause 保留画面、Stop 清屏；再次 Play 不重新下载。
图片位置与时长始终为零，只允许回到零的位置；没有自动播放结束事件。
替换正在播放的图片/视频会继续播放新资源；暂停时替换先准备黑屏，下一次 Play 才显示。
损坏图片会失败并清理，不会显示上一张图片冒充新资源。

请自行测试横竖 JPEG、带旋转/镜像方向的 JPEG、透明 PNG、图片与视频相互切换、
暂停/停止/重播和退出页面；反馈显示方向、比例、黑屏或内存异常，不上传私人照片。
限制与线程所有权见[ADR 0133](../adr/0133-android-dlna-images.md)。

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
Before running, verify `sys.boot_completed=1` and an up, non-loopback private
IPv4 interface on that isolated emulator. The normal foreground-host test
requires that interface; loopback-only cold boot is not a passing substitute.

The test creates original synthetic H.264 MP4/PCM WAV at runtime; no external
media, copied fixture or third-party decoder is included. It asserts real TextureView
pixels, control responses, actual player progress, source replacement/EOS and source
adapter bounds, FactoryDefaults output volume, TRACK_NR and replacement auto-resume.
A separate case starts the normal private-IPv4 foreground service
and verifies page-close teardown. Loopback bypass exists only in probe-feature native
builds; normal APKs cannot start it. See [evidence](../evidence/2026-10-15-dlna-playback/README.md).

Image cases generate original JPEG/PNG fixtures, exercise all eight JPEG EXIF
orientations, sample a 4000x3000 image, reject mismatched/truncated/oversized input
and cancellation, and assert actual UI screenshot pixels. The same suite exercises
photo/video/photo control boundaries, stale/closed display callbacks, malformed
image cleanup and normal foreground teardown. It does not inspect private media or
replace the owner-run physical/controller matrix.
