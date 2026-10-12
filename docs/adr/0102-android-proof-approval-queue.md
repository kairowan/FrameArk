# ADR 0102: bound Android approval between FANP proof and persistent trust

- Status: Accepted
- Scope: M2 Android Receiver trust boundary

## Context

Rust can verify an Ed25519 sender proof for one temporary FANP session, but the
Android UI must not silently turn that proof into persistent trust. The UI also
needs a bounded hand-off so a burst of verified peers cannot create unbounded
memory or leave stale approval material available indefinitely.

## Decision

Add `VerifiedPeerApprovalQueue` as a Kotlin boundary owned by the receiver
service/UI layer. It accepts only a key that the Rust transport has already
verified, copies the 32-byte public key, limits pending requests to 16, expires
requests after 60 seconds, rejects same-device key changes until an explicit
Rust revoke, and exposes explicit approve/reject operations through the
existing rollback-safe `TrustSettingsController`.

Persistence failure and an active receiver leave the request pending for a
bounded retry; rejection and expiry remove it without crossing into Rust trust
state. The queue stores no private key and does not verify signatures, invent a
fingerprint, auto-revoke a trusted device, or enable reconnect by itself.

## Consequences

- Android has a testable proof-to-approval lifecycle with replay, capacity, and
  TTL bounds.
- Rust remains authoritative for proof verification, canonical fingerprints,
  key rotation, and persistent registry mutation.
- A real Rust callback/pending-request transport and automatic reconnect policy
  remain separate M2 work and cannot be claimed from this boundary alone.
