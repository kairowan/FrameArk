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
