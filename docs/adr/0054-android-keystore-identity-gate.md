# ADR 0054: Android Keystore identity gate

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver startup

## Decision

Require the Android Receiver service to initialize and validate its Keystore-
backed device identity before starting the Rust lifecycle or foreground media
polling loop. `ReceiverIdentityCoordinator` retains only the public alias and
SHA-256 fingerprint returned by `AndroidDeviceIdentityStore`; private key
material stays in `AndroidKeyStore`. Keystore or identity failures leave the
service stopped and report only a redacted failure category.

This gate does not itself pair a sender or persist a Rust trust record. The
network/trust bridge remains a separate increment, and no fingerprint is
logged or exposed in the service notification.

## Verification

JVM tests cover one-time initialization, lower-case SHA-256 fingerprint
validation, stable failure categorization, and omission of provider error
messages. Android build/lint remains the required platform verification.
