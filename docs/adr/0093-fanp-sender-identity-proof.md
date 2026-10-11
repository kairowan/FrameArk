# ADR 0093: FANP optional sender identity proof

- Status: Accepted
- Scope: M2 FANP sender authorization foundation

## Context

The temporary PIN and pinned receiver certificate authenticate the pairing
channel, but they do not identify a sender across reconnects. The existing
post-PIN approval callback was intentionally address-only and could not safely
feed a trust UI.

## Decision

Add an opt-in FANP proof flow. After `PairAccepted`, the receiver sends a fresh
32-byte challenge. The sender signs a versioned transcript containing the
challenge and a bounded stable device identifier with an Ed25519 key held by
the caller's platform keystore. Rust verifies the public key/signature before
capability negotiation and exposes the verified `PeerIdentityProof` on the
temporary `PairingSession` for a caller-owned approval policy.

The proof uses separate one-frame QUIC streams for challenge, proof, and the
existing capability hello. Legacy pairing APIs remain unchanged. No private
key, persistent trust record, or Apple/Google pairing state crosses this
transport boundary.

## Consequences

- A receiver can distinguish possession of a sender key from knowledge of a
  displayed PIN before allocating media state.
- Android, desktop, and other platforms can delegate signing to their native
  keystore/keychain without moving private material into Rust.
- Registry integration, UI enrollment/revocation, platform sender clients,
  reconnect policy, and named-device interoperability remain follow-up work.
