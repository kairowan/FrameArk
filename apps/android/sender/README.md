# FrameArk reference Sender for Android

Experimental generated-pattern/tone source, not screen capture. Shares the Rust
JNI build with Receiver. Build: `./gradlew :sender:assembleDebug` after
`bash scripts/build_android_native.sh` (ARM64); normal APK has no test probe.

See [operator and developer instructions](../../../docs/testing/ANDROID_REFERENCE_SENDER.md),
[architecture/security](../../../docs/adr/0120-android-reference-sender.md), and
[compatibility](../../../docs/COMPATIBILITY.md). Foreground only, explicit
certificate confirmation and PIN, 30-second 720p30/1080p30 H.264 + AAC-LC.
No screen/microphone/storage permission, dependency, stored credentials or
automatic external intent connection was added.
