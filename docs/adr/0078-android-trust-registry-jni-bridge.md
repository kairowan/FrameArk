# ADR-0078: Expose the Rust trust registry through the Android JNI boundary

## Status

Accepted — experimental M2 security increment.

## Context

`frameark-trust` already provided bounded public-key fingerprints, explicit key
rotation rejection, revocation, labels, and capacity limits. Android had only
the local Keystore identity gate; a future approval/settings UI had no way to
use the Rust-owned registry without reimplementing trust policy in Kotlin.

## Decision

Add three additive JNI calls: `nativeTrustPeer`, `nativeRevokePeer`, and
`nativeIsPeerTrusted`. They accept only a bounded public device ID,
lowercase SHA-256 fingerprint, and label. The calls delegate mutation and
matching to a process-local `TrustRegistry`; Kotlin exposes thin wrappers and
validates only the public fingerprint/label shape needed to avoid crossing JNI
with obviously malformed input. Rust remains authoritative for identity-change
rejection, capacity, and revocation semantics.

No private key, pairing code, raw media, or certificate is passed through these
calls. They do not authenticate a sender, prove key possession, persist an
`FTR1` snapshot, or decide whether a human approved a request. A future Android
settings/trust UI must call these methods only after its explicit user approval
flow and must use Android-protected storage for any snapshot integration.

## Verification and limits

Rust tests cover same-key refresh, changed-key rejection, explicit revoke, and
unknown-record handling. Kotlin JVM tests cover forwarding and malformed-input
rejection before JNI. Physical Keystore, sender authentication, secure
persistence, and trust UI evidence remain open M2 work.
