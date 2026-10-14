# Android 短断与网络通知测试（Experimental）

本次修复的是：网络通知不再主动销毁现有 Receiver 连接、播放输出和配对信息。
不代表 Wi-Fi 切换、IP 变化或已失败会话已经支持自动重连。M2 仍未验收完成。

## 设备持有人测试

只在你自己的测试网络和设备上操作；不要中断他人共享网络。代理不连接或远程控制
你的设备。安装当前交付的 Receiver/Sender，参考[屏幕共享操作](ANDROID_SCREEN_CAPTURE.md)。

1. 两台设备在同一局域网，确认 TLS 指纹、PIN，开始播放非敏感测试画面和音频。
2. 若你有可仅影响这两台设备的路由器限流/丢包功能，可短暂阻断约 0.5–1 秒后恢复。
   没有这样的功能即可跳过，不要求 root、ADB 或安装网络拦截工具。
3. 记录画面是否恢复、是否有声音、暂停大约多久；检查 Receiver 的 PIN/端口是否
   未被回调自动刷新。不要把真实 PIN、连接票据或私密画面贴到工单。
4. 另一次测试中主动停止 Sender/Receiver，确认输出停止；再次启动必须重新核对
   当前指纹和配对材料。不要依赖切换网络来撤销授权，应明确停止 Receiver。
5. Wi-Fi 开关、切 AP/网络、IP 变化可单独记录，但不是同地址丢包等价测试；失败时
   明确停止后重新发现/确认/连接，不应无限重试或使用旧证书。

反馈：两设备型号/Android 版本、APK 版本、参考流或屏幕共享、音频选项、断网方式、
大致时长、恢复/失败/停止结果即可。现有队列与传输超时未放宽，高码率或更长断网仍可能
失败；本次没有新增自动降码率、过期帧清除或音画时钟重新同步。

## 自动化证据边界

```powershell
cargo test -p frameark-native --test network_continuity --locked
.\gradlew.bat --no-daemon :receiver:testDebugUnitTest :sender:testDebugUnitTest
```

- Rust：真实 pinned QUIC/TLS 经仅监听 localhost 的 UDP 丢包中继；在同一
  Offer/Start/媒体/Stop 会话内，连续两次全丢包 1 秒，检查帧字节/时间戳/顺序、
  会话保持和退出清理；持续断网到测试配置的 2 秒媒体读超时、任务取消均清理一次。
- JVM：通知不会 stop/start，20 次通知保持端点，停止/失败后通知无作用，快照失败
  不会暗中创建新会话。保留旧代码下失败的回归测试。
- `ReceiverNetworkContinuityTest`：开发者自有隔离模拟器，真实 JNI listener，
  注入 40 次合并后通知，比较内存中的证书/PIN/端口，显式停止/重启核对清理与换证。
  这是 JNI 生命周期测试，不是模拟器 Wi-Fi 切换或硬件解码测试。

这些测试均不能替代你的双真机网络、1080p60、A/V 同步和一小时稳定性记录。
设计及安全边界见 [ADR 0126](../adr/0126-retain-receiver-on-network-notifications.md)。
