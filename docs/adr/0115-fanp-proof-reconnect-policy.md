# ADR 0115: FANP proof reconnect admission policy

Status: Accepted

## Context

FANP proof mode already verifies an Ed25519 signature over a fresh server
challenge, then exposes an explicit Android decision before accepting a new
session. A sender that was already approved and stored in the bounded Rust
trust registry should be able to reconnect after an ordinary media/session
teardown without showing a new prompt. The policy must not silently accept a
rotated key or race Android trust mutations with a running listener.

## Decision

`frameark-ffi` gives each started listener a read-only `Arc<Mutex<TrustRegistry>>`
snapshot. For every verified proof, Rust derives the canonical fingerprint from
the proof public key and checks both the stable `DeviceId` and fingerprint.
Only an exact match is automatically admitted. Unknown peers and identity
changes continue through the existing bounded `FAPR` queue and explicit
decision callback. Trust snapshots are captured at startup; Android approve,
revoke, and restore operations remain stopped-only, so a revocation becomes
effective at the next lifecycle start. The proof challenge and signature are
still mandatory in every connection.

## Security and compatibility impact

This removes repeated prompts for an already trusted FANP sender while
preserving fail-closed behavior for key rotation, renamed IDs, malformed
proofs, and missing records. It is not Apple pairing, does not weaken the
temporary TLS certificate pin, and does not claim sender support on Android or
desktop. PIN-only mode remains the default for senders without FANP proof
support.

## Evidence

- `proof_policy_auto_accepts_only_an_exact_pretrusted_identity` covers exact
  matching and rejects key rotation and a renamed identity.
- `proof_enabled_network_runtime_auto_accepts_pretrusted_peer` covers the real
  pinned-QUIC listener loopback path and confirms no approval event is queued.
