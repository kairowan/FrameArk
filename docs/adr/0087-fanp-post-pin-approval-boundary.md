# ADR 0087: FANP post-PIN approval boundary

- Status: Accepted
- Scope: M2 Android/FANP trust integration seam

## Context

The FANP listener already verifies the displayed, process-local six-digit
pairing code. That proves control of the code, but it does not prove a sender
identity or key possession. A receiver still needs a bounded point at which a
caller-owned approval queue or UI can reject a connection before capability
negotiation and media state are created.

## Decision

`PairingServer::accept_pairing_with_approval` invokes a caller-owned async
policy after the pairing code is accepted and before `PairAccepted` is sent.
The policy receives only the QUIC peer address and returns a boolean. A false
decision sends the existing bounded rejection frame and returns
`TransportError::ApprovalRejected`; no `PairingSession` is created.

The callback is deliberately not an identity or signature verifier. Callers
must bind the address to an independently authenticated key-possession
transcript before presenting a persistent trust decision. Pending approval
requests remain ephemeral and are not included in FTR1 snapshots.

## Consequences

- Android and desktop adapters can connect a Rust-owned pending approval queue
  without moving protocol state into UI code.
- Rejection occurs before capability negotiation, media allocation, or trust
  persistence.
- The M2 transport-integrated authenticated approval and sender key-possession
  evidence gates remain open until a real authenticated peer transcript and
  device matrix are recorded.
