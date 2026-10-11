# ADR 0062: Android standby pairing display

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver standby UI

## Decision

The launcher Activity remains a small platform shell but now refreshes the
Rust-owned endpoint after starting/stopping the foreground service. When the
JNI bridge reports a valid endpoint it displays `frameark.local`, the
ephemeral port, and the six-digit pairing code, with an explicit refresh
button for delayed service startup. When the endpoint disappears it returns to
the ABI/status text. The certificate is never rendered, logged, or persisted.

This is a pairing affordance, not a completed authorization/trust UI. Sender
approval, remembered devices, QR provisioning, and physical-device playback
remain separate increments.

## Verification

Android lint, JVM tests, and debug APK assembly cover the Activity/resource
changes; JNI endpoint bounds are covered by `FrameArkNativeNetworkInfoTest`.
