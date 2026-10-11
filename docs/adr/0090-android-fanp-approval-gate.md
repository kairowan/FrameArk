# ADR 0090: Android optional FANP approval gate

- Status: Accepted
- Scope: M2 Android Receiver pairing UX

## Context

The Rust FANP transport now exposes a caller-owned post-PIN approval callback,
but the Android shell did not have a way to arm that boundary. Automatically
accepting a correct temporary PIN is useful for fixtures, yet it is not a
user-facing confirmation step.

## Decision

Add a disabled-by-default Android JNI gate. The UI can require approval for
future receiver pairings while stopped, then arm exactly one pairing after the
receiver is running. Rust consumes the arm immediately before `PairAccepted`;
the connection is rejected when no arm exists. Kotlin only exposes bounded
booleans and does not hold protocol state.

The gate is a temporary authorization affordance, not sender identity proof.
The PIN, peer address, and UI click must not be recorded as a persistent trust
grant. A future authenticated key-possession flow can use the same Rust seam.

## Consequences

- Android users have an explicit post-PIN confirmation path without moving
  handshake state into the Activity.
- Existing fixture and service behavior remains unchanged while the gate is
  disabled.
- Physical-device, sender identity, and hardware playback evidence remain
  required for the M2 exit gate.
