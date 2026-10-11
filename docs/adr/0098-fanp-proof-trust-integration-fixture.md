# ADR 0098: keep a loopback proof-to-trust integration fixture

- Status: Accepted
- Scope: M2 FANP trust regression coverage

## Context

FANP proof verification and Android trust enrollment are separate APIs. Unit
tests could validate each side while missing a change in the canonical public
key fingerprint or device-ID mapping between them.

## Decision

Add a loopback QUIC test that signs a real FANP challenge, verifies the proof,
enrolls the resulting public key through `TrustRegistry::trust_public_key`, and
asserts the stored record matches the verified device identity. The fixture
uses a deterministic test key and never represents user approval or a platform
sender matrix.

## Consequences

- Changes to proof framing or fingerprint formatting fail together in CI.
- No production transport dependency on the trust crate is introduced.
- Android UI, secure keystore possession, automatic enrollment, and reconnect
  behavior remain explicit follow-up work.
