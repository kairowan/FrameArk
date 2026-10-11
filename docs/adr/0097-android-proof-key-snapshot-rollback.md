# ADR 0097: persist proof-key enrollment through the Android trust controller

- Status: Accepted
- Scope: M2 Android trust settings

## Context

The JNI boundary can now derive a canonical trust fingerprint from a raw
proof-verified public key, but persistence must remain coupled to explicit UI
approval and encrypted snapshot storage. A failed file replacement must not
leave the in-memory Rust registry ahead of the durable Android state.

## Decision

`TrustSettingsController` accepts `VerifiedPeerKey` alongside the legacy
formatted-fingerprint type. It snapshots the current Rust registry, applies the
explicit raw-key mutation, persists through `AndroidTrustSnapshotStore`, and
restores the previous snapshot on persistence failure. The service exposes the
same operation for already-approved callers.

The controller does not verify FANP signatures, infer user approval, or enable
automatic reconnect. The existing stopped-receiver guard remains mandatory.

## Consequences

- Proof-key enrollment now has a tested Android persistence/rollback seam.
- Rust remains authoritative for canonical fingerprints and key rotation.
- Real sender UX, proof callback plumbing, and physical-device evidence remain
  open M2 work.
