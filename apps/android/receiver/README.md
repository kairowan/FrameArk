# FrameArk Receiver (experimental Android shell)

Build the Rust JNI library and APK together. The production build currently
packages `arm64-v8a`; `scripts/build_android_native.sh` and the Android CI job
perform native packaging. This shell is not a completed hardware playback MVP.

## Verified FANP session approval

1. Stop the receiver, then select **Require verified sender proof**.
2. Start the receiver service. Keep the app in the foreground while connecting
   a sender that implements FANP's Ed25519 proof handshake. The fixture CLI's
   PIN-only sender cannot connect in this mode.
3. For a new identity the app shows its claimed device ID and SHA-256 public-key
   fingerprint after Rust verifies key possession. Compare the fingerprint
   with the expected sender: the displayed name alone is not authenticated.
4. Select **Approve this proof session** or **Reject this proof session**.
   Approval allows only this connection. It does not save a trusted device or
   modify the encrypted registry. Previously enrolled exact identities may
   reconnect without a new prompt.
5. To use a PIN-only sender again, stop the receiver and explicitly select
   **Use PIN-only pairing (while stopped)**. Mode selection is process-local,
   not a persisted security preference.

Requests expire after 30 seconds in Rust. The foreground UI refreshes every
250ms and removes cancelled, expired, decided or stopped requests. Leaving the
page (including screen lock/backgrounding or Activity recreation) rejects the
currently displayed request; undisplayed requests expire in Rust. A sender must
retry with a new handshake if a request expires. No keys or prompts are saved
in Activity state or logged. Approval buttons ignore obscured touches.

The older **Approve next pairing** control applies only to the separate
PIN-only confirmation gate; it is not a decision about a displayed proof.
Persistent trust settings are separate and require the receiver to be stopped.
Live trust revocation and a complete proof-capable sender UX remain follow-up
work.

Rust enforces this stopped-only rule even for direct JNI callers. Wait for
stop to finish before changing trust; running/stopping mutations return failure,
not a live revocation guarantee. See [ADR 0118](../../../docs/adr/0118-native-trust-lifecycle-gate.md).

Device owners can follow the [manual APK checklist](../../../docs/testing/ANDROID_MANUAL_TESTING.md)
without granting remote access. Actual device evidence is recorded only after
their test results are received.

## Verification

`./gradlew :receiver:testDebugUnitTest :receiver:lintDebug` covers the Kotlin
bridge, single-session prompt coordinator, stale clicks, expiry, denial and
background dismissal. `cargo test -p frameark-ffi --lib --locked` covers real
pinned-QUIC proof admission after a delayed decision, queue expiry and cleanup.
Neither substitutes for physical-device visual, decoder, network-switch or
one-hour playback tests in `PLAN.md`.

The [Android 14 emulator smoke record](../../../docs/evidence/2026-10-13-proof-ui/README.md)
includes the idle-screen screenshot, real JNI loading and mode/start/stop
checks, with the remaining incoming-sender integration limitation stated.

## Native video/audio preview

The foreground TextureView now receives actual H.264 output through Rust's
ordered FAPE queue. AAC-LC and constrained Opus decode to PCM16/AudioTrack.
The screen reports video submissions, frame-rendered callbacks and completed
PCM buffer writes (not measured audible output). Destroying an active surface
stops the service; an idle listener may remain while sharing a pairing ticket.
Keep this page foreground; background playback and rotation
continuation are not implemented. Codec setup/backpressure failure stops playback
and reports a static source location without recording media or peer details.

Only Annex-B H.264 with initial SPS/PPS/keyframe, raw AAC-LC, or 48 kHz mapping-0
Opus mono/stereo are in this experimental path. See
[ADR 0119](../../../docs/adr/0119-android-ordered-native-playback.md) for the
timestamp/API migration, memory budgets and unsupported configurations.
A separate reference Sender APK now supplies generated pattern/tone content;
see the [two-APK operator guide](../../../docs/testing/ANDROID_REFERENCE_SENDER.md).
Real capture, discovery UI and reference-device 1080p performance remain open.

### Opt-in emulator instrumentation (developers only)

Use a disposable API 34 x86_64 AVD, not a device owner's phone. With the Rust
Android target and NDK linker configured as in the native build script, build
`frameark-ffi` for `x86_64-linux-android` with the explicit
`--features android-playback-probe` flag. Copy its `.so` to
`build/probe-jni/x86_64/libframeark_ffi.so` in the repository root, then run:

```sh
./gradlew :receiver:assembleDebug :receiver:assembleDebugAndroidTest -PframearkPlaybackProbe=true
adb -s emulator-5582 install -r apps/android/receiver/build/outputs/apk/debug/receiver-debug.apk
adb -s emulator-5582 install -r apps/android/receiver/build/outputs/apk/androidTest/debug/receiver-debug-androidTest.apk
adb -s emulator-5582 shell am instrument -w -e class dev.frameark.receiver.NativePlaybackTest dev.frameark.receiver.test/android.test.InstrumentationTestRunner
```

Replace the example serial with your disposable emulator's serial. The test
creates gray 320×240 H.264 and silent 48 kHz stereo AAC with Android encoders,
sends 90 video frames plus paced audio over real pinned loopback QUIC, checks
platform render/write callbacks, Stop output release and service listener
release, and saves `native-playback.png` in the app's external-files directory.
The probe exists only in explicitly opted-in Rust builds and test sources;
never enable it in a release APK. Omit the Gradle property and rebuild normal
ARM64 JNI before packaging an owner-test APK. This test does not cover Opus,
physical decoding, audible quality, LAN discovery, one-hour stability or sync.
