# ADR-0080: Encrypt Android trust snapshots at the platform boundary

## Status

Accepted — experimental M2 security increment.

## Context

`frameark-trust` owns bounded trust policy and the versioned `FTR1` snapshot,
but its generic file store cannot choose Android permissions or key protection.
The JNI bridge also had no way to restore a remembered registry without
duplicating trust semantics in Kotlin.

## Decision

Expose additive `nativeExportTrustSnapshot` and `nativeImportTrustSnapshot`
methods. Rust encodes and validates the complete registry, caps the bytes, and
rejects replacement while a session or listener is active. Android wraps the
public bytes in an `FATS1` AES-GCM envelope, obtains its AES key from
`AndroidKeyStore`, writes to an app-private file with `fsync` and an atomic
same-directory move, and restores before the receiver can start.

The adapter never persists private keys, pairing codes, or raw media. It does
not authenticate peers, prove possession of a fingerprint key, or replace the
future settings/approval UI. Hardware-backed Keystore protection remains
device-dependent; a platform that cannot create or use the key fails closed.

## Verification and limits

Rust tests cover FTR1 export/import and active-session replacement rejection.
Android JVM tests cover encrypted round trip, tamper rejection before JNI
import, and bounded provider forwarding. Physical Keystore hardware, backup
restore behavior, and user approval UX remain open evidence items.
