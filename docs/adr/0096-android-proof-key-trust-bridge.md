# ADR 0096: expose proof-key trust enrollment through the Android boundary

- Status: Accepted
- Scope: M2 Android receiver and FANP trust seam

## Context

Rust can now derive one canonical trust fingerprint from a verified FANP
Ed25519 public key, but the Android approval surface only accepted a caller
formatted fingerprint. That made it easy for a future proof callback to
duplicate formatting or accidentally enroll a key before its approval state was
clear.

## Decision

Add `nativeTrustPeerPublicKey` and the matching Kotlin
`FrameArkNative.trustPeerPublicKey`/`VerifiedPeerKey` entry point. The Android
caller supplies a bounded device ID, raw 32-byte public key, and display label
only after Rust transport proof verification and explicit user approval. Rust
derives the fingerprint and applies the existing bounded registry policy;
silent key rotation and capacity limits remain enforced.

The entry point does not verify signatures, infer approval, persist snapshots,
or bypass the existing string-fingerprint API. Platform code remains
responsible for keystore possession, user interaction, and snapshot persistence.

## Consequences

- Android and FANP share one fingerprint representation without Kotlin crypto
  or duplicated formatting.
- The proof-to-registry lifecycle is now a usable boundary, but not an
  automatic enrollment or reconnect policy.
- Raw public keys remain public metadata; private keystore material never
  crosses JNI.
