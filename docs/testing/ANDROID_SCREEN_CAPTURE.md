# Android 屏幕共享预览 / Experimental screen capture

发送端需要 Android 10+ ARM64，接收端需要 Android 8+ ARM64；两台设备在同一可信
局域网。你自行安装与测试，只需反馈脱敏结果，不需要开放 ADB、远控或提供私人媒体。
这不是 AirPlay，也不是已完成的一小时/1080p60 Beta 验收。

## 操作

1. 安装同一构建的 Sender 和 Receiver APK。预览使用测试签名，不是商店正式版。
   签名冲突时不要直接卸载丢失数据；先报告冲突并决定如何保留旧数据。
2. 在 Receiver 启动接收，保持接收界面前台；Sender 默认 PIN 模式，也可在两端明确
   启用[Keystore 身份验证](ANDROID_SENDER_IDENTITY.md)。若开启一次性配对确认，先允许下一次配对。
3. 分享不含 PIN 的连接信息，在 Sender 粘贴、检查，核对两端完整 TLS 指纹与地址。
   确认后单独输入接收端 PIN。不要把这些信息发给开发者。
4. 初次建议 720p、30 fps、仅屏幕。点击“授权并共享屏幕”，同意通知权限，阅读并
   确认 Android 系统共享提示。可取消，不会暗中开始或自动重试。
5. 切到普通、非敏感应用。Sender 的凭据页面自身防截屏，停在该页可能是黑屏，
   不代表其他应用不能共享。受保护内容出现黑屏是系统安全行为，不能绕过。
6. 可选重新开始时勾选“系统播放音频”。Android 会请求音频权限，但 FrameArk 不采集
   麦克风。仅允许捕获的同一用户配置文件中的媒体/游戏音频可共享；应用禁止捕获、
   通话等情况不能保证有声音。没有音频时不会退回麦克风。
7. 从通知或 Sender 点击停止，确认接收端画面/音频结束、通知消失。重复三次；再测试
   锁屏停止与系统停止共享。每次重新开始都需要重新输入 PIN、确认指纹与系统授权。
8. 基础通过后分别尝试 1080p、60 fps 请求值与更长运行，并记录实际表现。最多 1 小时。
   旋转保持本次固定编码画布，可能黑边；网络断开需手动重新配对，不宣称自动恢复。

降低接收端音量再播放自有测试音频；不要用私人消息、密码、金融或受保护媒体测试。
PIN 失败、授权被拒、编码不支持或网络错误会失败，不会自动降级 TLS 或画质。

## 反馈

提供构建名、两端型号/系统版本、所选分辨率/FPS、是否勾选音频、阶段/源代码位置及
数字错误码、运行时长、画面/声音观察、停止/锁屏/重复结果。可报告界面视频、音频包、
非静音 PCM 计数；这些不是接收端实际 FPS、听感、延迟或 A/V 同步测量。
不要提供 PIN、连接信息、指纹、IP、私人截图、未脱敏日志或设备唯一标识。

## Developer reproduction

Use the same two isolated API 34 emulators and temporary host UDP forwarding
described in [reference testing](ANDROID_REFERENCE_SENDER.md). Build with
`-PframearkPlaybackProbe=true` and Rust `android-playback-probe` for x86_64 only.
Run `scripts/test_android_reference_sender.ps1` with the required emulator/ADB
arguments and `-Capture` or `-CaptureVideoOnly`. The latter locks the disposable
sender emulator and asserts automatic stop, then wakes it. No physical serials
are accepted. System UI consent is clicked explicitly, never bypassed with
PROJECT_MEDIA app-ops. RECORD_AUDIO/notification grants are test setup only.

The opt-in debug CaptureFixtureActivity draws an actual changing OS screen and
plays an AudioTrack tone. MediaProjection captures it; Receiver asserts decoded
red/blue pixels, not merely queued frame counts. Audio mode additionally asserts
non-silent source PCM and receiver PCM writes. Video-only asserts zero audio.
Both assert stop/release. Normal APKs exclude the fixture and probe-native code.
See [evidence](../evidence/2026-10-13-screen-capture/README.md).
