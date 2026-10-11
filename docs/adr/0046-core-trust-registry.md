# ADR 0046: bounded core trust registry

- Status: Accepted
- Scope: M2 receiver trust and M4 AirPlay pairing boundary
- Date: 2026-10-11

## Context

Android already exposes a Keystore-backed P-256 identity primitive, while the
Rust protocol layers need one policy for remembered devices, explicit key
rotation, revocation, and bounded storage. Keeping this logic in each platform
would allow inconsistent reconnect decisions and could silently accept a new
key under an old device identifier.

## Decision

Add `frameark-trust` with an in-memory `TrustRegistry`. Records contain a
`DeviceId`, a lowercase colon-separated SHA-256 public-key fingerprint, and a
bounded display label. A same-key reconnect refreshes the label; a different
fingerprint is rejected until the caller explicitly revokes the record. The
registry has a bounded capacity and emits no secret material in `Debug` output.

The registry also exposes a deterministic bounded `FTR1` snapshot codec. The
codec is a serialization contract only; it does not encrypt or authenticate a
snapshot and must be wrapped by the platform's secure storage.

The crate does not generate keys, verify signatures, persist records, or
perform user-facing PIN/TV confirmation. Android, desktop, and future daemon
adapters remain responsible for secure persistence, cryptographic verification,
approval UI, and recovery from storage errors.

## Consequences

- FANP, AirPlay, and other adapters can share one deterministic trust decision.
- Silent identity replacement is rejected and key rotation is auditable as an
  explicit revoke-then-trust operation.
- The registry alone is not a pairing implementation and cannot be advertised
  as persistent trust until a platform-backed store is integrated and tested.
