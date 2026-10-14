# ADR 0125: Android Sender Keystore proof

- Status: Accepted for Experimental use
- Scope: M2 sender identity/authentication UX, not a milestone exit gate

## Decision

Use a persistent, app-scoped P-256 key in Android Keystore (Android 8+ baseline).
The key is created only by the user's load/create action, uses `PURPOSE_SIGN`
and `SHA256withECDSA`, and is never exported. Existing unreadable/invalid keys
fail closed; they are not deleted or rotated automatically. Uninstall/data
reset can lose the identity. No biometric, StrongBox, attestation, or universal
hardware-backed storage claim is made.

Rust owns SPKI normalization, device ID, fingerprint, proof transcript and
verification, trust matching and admission. Kotlin owns OS key calls and UI.
No new crypto implementation or dependency is introduced. Sources:
[Android KeyGenParameterSpec](https://developer.android.com/reference/android/security/keystore/KeyGenParameterSpec)
and [ring P-256 ASN.1 verification](https://docs.rs/ring/0.17.14/ring/signature/static.ECDSA_P256_SHA256_ASN1.html).

## Versioned wire/API

FANP transport version/ALPN remain 1. The optional proof payload gains version
2: `02 | id_length:u8 | id | key:65 | signature:8..72`, max 203 bytes, beneath
the existing 256-byte frame cap. ID is 1..64 ASCII `[A-Za-z0-9_.-]`. Key is
uncompressed SEC1 P-256 (`04 || X || Y`); signature is ASN.1 DER ECDSA/SHA-256,
strictly verified by ring. Length checks alone do not authenticate a key.
Unknown versions, compressed keys, malformed DER, invalid points/signatures,
wrong keys or stale challenges reject before policy/media.

V2 signed bytes are the ASCII domain `FrameArk FANP peer authentication v2
P256-SHA256` (one space, no newline), one NUL byte, the one-byte ID length,
ID bytes, 65-byte public key, and fresh 32-byte server nonce. This separates
v1/v2 and binds algorithm, key, identity and challenge. Ed25519 v1 bytes and
the synchronous `connect_with_identity` API stay unchanged. The pre-alpha
`PeerIdentityProof.public_key/signature` Rust fields become `Vec<u8>`; callers
with fixed arrays migrate using `.to_vec()`. New `PlatformPeerIdentity` and
`connect_with_platform_identity` accept a bounded asynchronous signer.

Android supplies exactly 91-byte DER SPKI with the canonical id-ecPublicKey /
prime256v1 prefix and 65-byte point. Rust rejects other encodings, strips the
26-byte prefix, hashes the canonical raw key with SHA-256, and derives the
device ID `sender-` plus the first 32 lowercase hex fingerprint characters.
The displayed/trusted fingerprint remains the **entire** colon-separated digest.
It is distinct from the receiver's ephemeral TLS and own SPKI fingerprints.

Receiver FAPR keeps its 14-byte header. Version 1 means 32-byte Ed25519 key;
version 2 means 65-byte P-256 key. Exact length is mandatory. FTR1 snapshots
already store fingerprints, so need no migration. Matching uses full device
ID and fingerprint after a fresh valid proof, still requires PIN, and applies
only the listener-start trust snapshot. No automatic enrollment or live revoke
of an active session is added; stop reception before changing trust.

## Thread, lifecycle and abuse limits

One worker generation has one signing bridge: idle -> pending -> closed. Only
one transcript (<=256 bytes) can be polled once, and one 8..72-byte signature
can be submitted after polling. JNI checks array lengths before copying and
checks worker generation, connecting status and cancellation before acceptance.
Timeout/drop closes the bridge; late/duplicate/stale replies cannot authorize.
The existing encoder/capture worker calls Keystore; Rust never makes a blocking
JVM callback. One key-load and one signing OS operation are admitted per process.
An OS signing call cannot be forcibly interrupted, but Rust continues to poll
cancellation (50 ms), drive QUIC and enforce deadlines independently. A stuck
OS call keeps its slot, preventing unbounded replacement signing threads.

Identity setup has a 60-second total Rust cap, 40-second per-operation cap;
Receiver's explicit decision still expires after 30 seconds. Kotlin's 65-second
wait is only a UI/producer backstop. Reference worker lifetime becomes 120 seconds
in proof mode (30-second media unchanged); capture keeps its 3660-second cap.
Media budgets, queue limits and one-hour capture limit are unchanged.

## UX, rollout and rollback

Identity mode is explicit/default-off each Activity session for legacy peers.
Load/create shows Rust-derived full sender ID/fingerprint, then allows the
checkbox. Mode changes clear PIN and TLS confirmation. Sender snapshots the
choice for reference/capture; capture carries only an opaque key handle via
the existing process-memory handoff, never Intent/disk. Receiver must enable
proof-required mode while stopped. Existing allow/deny UI displays the same
canonical sender fingerprint. Failures never retry as PIN-only or Ed25519.
Users can explicitly start a new legacy session with both proof modes off;
this does not count as authenticated-sender acceptance.

Old receivers reject v2 and old Senders cannot use the new proof flow. Use
matching APKs. Reverting this increment retains v1/PIN compatibility but removes
P-256 admission; stored fingerprints must not imply proof support. No production
APK signing key, release, package dependency, or repository protection changes.

## Evidence and remaining gates

Tests cover v1 regression, v2 bounds/mutation/replay/domain/wrong-key rejection,
real QUIC approve/deny/invalid signature, asynchronous signer timeout, one-use
bridge cleanup, JVM FAPR v2 bounds, and disposable Android Keystore/JNI/QUIC
integration. See the [operator guide](../testing/ANDROID_SENDER_IDENTITY.md) and
[recorded evidence](../evidence/2026-10-14-sender-identity/README.md).
Physical Android versions/vendors, durable key behavior across device restart,
live revocation, recovery, and 1080p60/one-hour M2 performance remain open.
