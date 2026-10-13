# Android 双 APK 受保护签名 / Protected release signing

生产签名采用维护者选择的 GitHub Environment secrets，不把私钥放入仓库、聊天、
PR、普通 CI artifact 或构建日志。当前配置的是 `android-release` 环境；没有自动
生成或上传生产密钥。普通 CI 仅使用即用即删的独立测试密钥，不能用于正式更新。

## 维护者首次配置

在 [GitHub Environments](https://github.com/kairowan/FrameArk/settings/environments)
打开 `android-release`。直接在那里填写，不要在聊天中发送值：

| 类别 | 名称 | 内容 |
|---|---|---|
| Environment secret | `ANDROID_KEYSTORE_BASE64` | 已备份生产 JKS/PKCS12 keystore 的单行 Base64，最多 48 KiB |
| Environment secret | `ANDROID_KEYSTORE_PASSWORD` | keystore 密码 |
| Environment secret | `ANDROID_KEY_ALIAS` | 两个 APK 使用的同一密钥 alias |
| Environment secret | `ANDROID_KEY_PASSWORD` | 该私钥的密码（PKCS12 通常与 store 密码一致） |
| Environment variable | `ANDROID_SIGNING_CERT_SHA256` | 独立核对的签名证书 SHA-256，64 位十六进制，无冒号 |

签名证书指纹不是秘密，但必须独立核对。可由维护者在自己的受信任电脑运行
`keytool -list -v -keystore <本地 keystore> -alias <alias>`，交互输入密码后核对。
不要复制私钥/密码到命令行参数、工单或日志；不要把当前 Android Debug 证书当作
生产身份。生产密钥的创建、离线备份、恢复与轮换由维护者保管；本流程不替代密钥恢复方案。

当前保护规则：仅 `v*` **tag**，需要 `kairowan` 明确批准部署，禁止管理员绕过。
单维护者可以批准自己发起的部署（`prevent_self_review=false`），否则唯一维护者
无法发布；这不是不经确认就释放 secrets。加入第二位维护者后应开启防自批。
环境设置不由 YAML 自动保证：每次正式发布前再次审计环境与 tag 规则。

## 发布步骤

1. 在专用分支完成版本/兼容性/升级说明，PR 的 CI、安全审核通过后合并 main。
2. 确认 **main 上对应提交** 的 `ci-gate`、`security-gate` 已完成成功。
3. 维护者选择版本，在该提交创建 annotated 或 signed SemVer tag 并推送。
   现阶段应使用 `v0.x.y-alpha.N` 等预发布名；不要为了试流水线就发布正式版本。
4. Tag push 会运行 Release。手动重跑时必须选择同一个 tag 作为 workflow ref，并
   在 `tag` 输入填写该 tag；不能在 main/任意分支选择另一个 tag 绕过环境限制。
5. 无密钥 build job 核对 annotated tag、checkout、main ancestry 和 main checks，
   执行 Rust/Android 测试，构建两个 `*-release-unsigned.apk`。校验应用 ID、版本、
   非 debuggable、仅 ARM64 原生库、无 probe/fixture，并上传不可变 artifact ID。
6. 查看构建结果与发布范围，然后亲自批准 `android-release` 环境。签名 job 使用
   相同 commit 与 artifact ID，不重新运行 Gradle；它先检查版本身份和发布通道，
   再临时解码密钥，依次 align、sign、verify 两个 APK，逐个核对证书 SHA-256。
7. 仅完整成功的双 APK 集合进入最终目录；生成签名清单、校验和和 OIDC provenance，
   然后 GitHub Release 发布全部最终文件。缺 secrets、错误密码/指纹、签名/校验失败
   都停止，无 debug 签名回退，无 unsigned APK 冒充安装包，也不覆盖既有 release。

稳定版还要求环境 variable `STABLE_RELEASE_APPROVED=true`；默认不设置即禁止 stable。
仅在 PLAN 的稳定性、兼容性、升级、许可证/专利审查与完整发布证据通过后显式开启，
该开关不是自动证明已完成里程碑。当前 M0–M5 尚未全部验收，不应开启。

APK `versionName` 来自 tag 去掉 `v`；`versionCode` 为该提交在 main 的 first-parent
提交数（完整历史，受保护 main 不重写），不是写死的 1。后续可升级版本必须使用更晚的
main 提交；不要给同一提交打多个用于应用更新的版本 tag，不要移动/重用已发布 tag。
本地默认构建仍为 0.1.0/code 1，保持开发用途，不会暗中使用生产 secrets。

## 产物及边界

正式分发目录包括 Receiver 与 Sender 签名 ARM64 APK、`android-signing.json`
（tag/commit/versionCode/证书指纹/两包哈希）、source ZIP、Rust SPDX SBOM、项目
许可证、当前兼容性说明和 `SHA256SUMS`。OIDC 证明构建来源，不替代 APK 签名。
Rust SBOM 不代表 Android SDK、平台编解码器或地区专利的全面法律结论。
自动收集全部第三方许可证/NOTICE 的分发包仍待补齐（包括 Android runtime 依赖）；
在完成该项及分发审查前，不应批准实际发布，预发布也不能免除许可证义务。

**当前本地预览使用 Android Debug 证书。** 正式密钥不同会导致不能覆盖安装；需
提前选择迁移方式并保留数据，不要要求用户直接卸载丢失信任配置。当前没有跨签名
自动迁移承诺，也没有 Play 商店密钥托管/轮换支持。密钥遗失不能由重跑 CI 修复。

## 可复现验证

`python -m unittest discover -s scripts -p 'test_*.py'` 验证 tag/环境/输入哈希/
证书/平台边界。CI 的 Android job 还构建 `0.0.0-ci` / code 2 的实际 unsigned
双 APK，运行 `scripts/android_release_smoke.py --sdk-tools <SDK build-tools/35.0.0>`。
它生成两日有效的独立 fixture 密钥，真实签名、验证两包，测试错误证书、错误密码、
缺 secret 与失败清理，并删除测试私钥和测试产物。不会上传测试签名 APK 给用户。
Windows PowerShell 下 Gradle 参数应整体加引号，例如 `'-PframearkVersionName=0.0.0-ci'`。

没有生产 secrets 和明确发布版本时，不创建 tag、触发正式签名或声称生产发布已验收。

参考：[Android apksigner](https://developer.android.com/tools/apksigner)、
[GitHub protected environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)。
