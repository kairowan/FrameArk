# Network-notification continuity — 2026-10-14

Scope: fix the Android observer's destructive restart; keep the authenticated
Rust transport alive within existing budgets. Branch `fix/android-network-continuity`.
No wire/ABI/dependency/production-key changes. M2 remains Partial.

## Reproducer and tests

- Before the fix, `ReceiverSessionCoordinatorTest.network_change_preserves_native_and_playback_owners`
  failed with extra loop-stop/native-stop/native-start/loop-start calls.
- After the fix, 94 JVM tests and both debug Lint tasks pass. Tests include 20
  repeated notifications, missing endpoint snapshot, stopped/failed lifecycle,
  callback coalescing and playback-error cleanup.
- `cargo test --workspace --all-features --locked`: 214 passed, one opt-in
  live-mDNS test intentionally ignored. `cargo fmt --all -- --check`, all-target/
  all-feature Clippy with `-D warnings`, and rustdoc with warnings denied pass.
- `network_continuity` integration: real pinned QUIC/TLS through a single-peer
  loopback UDP relay; all datagrams dropped during two one-second blackouts.
  Eight total FAM1 frames (four opaque H.264-labelled video, four AAC-labelled
  audio) retain exact bytes/order/timestamps through one Offer/Start/Stop.
  Actual dropped-packet counters and absence of mid-stream renderer resets are
  asserted. The test does not simulate a new handshake or decode codecs.
- Negative cases: 2-second test-configured media read timeout fails closed and
  cleans both outputs/backend exactly once; task cancellation during loss also
  cleans without waiting for traffic. Production timeouts are unchanged.
- Python repository/release tests: 22 passed.

## Android JNI evidence

Agent-owned API 34 x86_64 emulator `frameark_proof_ui`, isolated port 5582;
the owner's existing device/emulator was not used. Normal Rust x86_64 JNI library,
test-only APK packaging switch, no production signature.

`ReceiverNetworkContinuityTest.testRouteNotificationsKeepRealListenerAndExplicitStopStillReleasesIt`
passed (one test, 0.519 seconds). Forty injected/coalesced notifications retain
the actual Rust listener certificate/PIN/port and do not stop the fake polling
owner. Explicit stop clears real JNI state, late events do not restart, and an
explicit new start creates a different certificate. No endpoint material logged.

This is real JNI plus injected callbacks, not actual Android network switching,
MediaCodec output, two-device Wi-Fi recovery, full-rate capture, latency or soak.
Physical testing remains owner-operated through [the test guide](../../testing/ANDROID_NETWORK_CONTINUITY.md).
