# ADR 0095: derive trust fingerprints from verified FANP proof keys

- Status: Accepted
- Scope: M2 FANP and trust-registry integration seam

## Context

FANP now verifies an optional Ed25519 sender proof and exposes its temporary
`PeerIdentityProof`. The trust registry stores canonical lowercase,
colon-separated SHA-256 public-key fingerprints, but callers had to derive
that representation themselves before enrollment. Duplicated formatting would
make reconnect and snapshot comparisons error-prone.

## Decision

`frameark-trust` derives the canonical fingerprint from a raw 32-byte public
key and exposes `TrustRegistry::trust_public_key` as one bounded enrollment
operation. The helper is intentionally a hash/format bridge: it does not
verify signatures, infer labels, persist snapshots, or approve a peer. A
transport or platform adapter may call it only after FANP proof verification
and explicit user approval.

## Consequences

- FANP and Android trust adapters share one deterministic fingerprint format.
- Silent key rotation remains rejected by the existing registry policy.
- No private key or pairing code enters the registry or its snapshot.
- Automatic enrollment, sender UX, and reconnect policy remain future work.
