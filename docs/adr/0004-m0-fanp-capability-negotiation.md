# ADR 0004: M0 FANP capability negotiation and core session mapping

- Status: Accepted
- Date: 2026-10-09
- Decision owners: FrameArk maintainers

## Context

Temporary pairing proves that a peer controls the displayed code, but it does
not say whether the two endpoints can carry video, audio, subtitles, or remote
control. The core already owns the canonical capability and session models;
transport code must not create a second lifecycle or capability vocabulary.

## Decision

1. Add a bounded `ClientHello`/`ServerHello` exchange on a second reliable
   QUIC bidirectional stream after pairing.
2. Encode the capability schema as a version byte, an entry count capped at
   16, and sorted unique one-byte IDs. IDs map directly to the canonical
   `frameark-core::Capability` inventory.
3. Select the set intersection. No common capability is an explicit
   negotiation failure; unknown IDs, duplicates, truncated payloads, and
   unsupported schema versions are rejected before interpretation.
4. Expose the selected `NegotiatedCapabilities` on the temporary transport
   session. Provide an adapter hook that advances the shared core `Session`
   only through valid `Connecting`, `Authenticating`, and `Negotiating` states.
5. Keep media tracks, codec profiles, dimensions, frame rates, and datagram
   policy for a later `SessionOffer/SessionAnswer` increment.

## Consequences

- Android, desktop, and headless implementations share the same capability
  IDs, intersection rules, and state transitions.
- The handshake can reject an incompatible endpoint before any media resource
  is allocated.
- A capability offer is not a device identity or trust record. Persistent
  identity, replay policy, and media negotiation remain separate milestones.
- The current schema is experimental and intentionally small; adding a new
  capability requires a core-model, protocol-spec, compatibility, and test
  update together.

## Security notes

The offer is carried inside the already pinned TLS connection and is bounded
before allocation. Version equality is required, so this increment does not
silently downgrade to an unknown schema. Negotiation does not authorize remote
input, file transfer, internet relay, or protected-media access.
