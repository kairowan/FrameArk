# ADR-0074: Bind Android Keystore identity to the Rust receiver session

## Status

Accepted — experimental M2 increment.

## Context

The Android receiver already creates a persistent EC key pair in Android
Keystore and validates a redaction-safe alias and public-key fingerprint. The
Rust JNI lifecycle previously ignored that identity and created sessions with a
fixed `android-device` identifier. The local Keystore alias also defaults to the
same label on all installations, so neither constant can uniquely name a device.

## Decision

`FrameArkNative.setDeviceIdentity` loads the native library, validates the same
alias/fingerprint policy on the Kotlin side, and forwards only those public
values to `nativeSetDeviceIdentity`. Both sides require an ASCII lowercase
alias of at most 64 bytes. Rust validates the lowercase colon-separated SHA-256
format, stores the fingerprint in process-local state, and derives a device ID
as `sha256-` plus its 64 hex characters. Both the lifecycle `Session` and FANP
network-session use that ID. `nativeStartReceiver` returns `-5` until an
identity is installed. Reinstalling the same key is idempotent for Service
recreation; replacing it while a session is active returns `-2`. Invalid input
returns `-3` without changing the installed identity. The alias is retained
only as redaction-safe process-local metadata for future trust records.
Stopping clears the
session and media state but preserves the public identity for recovery.

The private key never crosses JNI and is not serialized into configuration or
logs. This increment does not claim completed user-facing trust UI, pairing
verification, or cryptographic peer authentication; those remain M2/M4 work.
The Keystore key does not sign the ephemeral TLS certificate in this increment.
The bundled shell/library must be updated together because native startup now
requires identity installation; the core ABI version itself is unchanged.

## Consequences

- Rust and Android now share one stable device identity for session/trust work.
- A missing Keystore identity fails closed before the receiver publishes a
  network endpoint.
- The fingerprint is retained for the upcoming trust registry integration, but
  is not yet a proof of peer authentication by itself.
- Tests cover strict fingerprint formatting and Kotlin bridge rejection before
  JNI invocation; physical Keystore and device interoperability evidence remain
  outstanding.
