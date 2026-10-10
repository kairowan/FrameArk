# ADR 0016: Android Keystore device identity

- Status: Accepted
- Date: 2026-10-10
- Scope: M2 Android Receiver trust boundary

## Decision

`AndroidDeviceIdentityStore` creates or loads one P-256 signing key pair from
`AndroidKeyStore`. The private key is never returned to Kotlin callers or
written to preferences/files. The Rust trust boundary receives only the stable
alias and a colon-separated SHA-256 fingerprint of the certificate public key.
Aliases are bounded and restricted to lowercase device-safe characters before
any Keystore operation.

This is an identity primitive, not a completed pairing system. PIN/TV
confirmation, trust persistence/revocation, key rotation, and native FFI
exchange remain separate work. The JVM tests cover deterministic policy and
fingerprint formatting; real Keystore provisioning must be verified on Android
devices because the JVM test runner has no Android Keystore.
