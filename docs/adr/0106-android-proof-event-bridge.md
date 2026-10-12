# ADR 0106: Bridge verified FANP proof decisions through bounded JNI events

- Status: Accepted
- Date: 2026-10-12
- Milestone: M2

## Context

FANP already verifies an optional Ed25519 sender proof in Rust, but the
Android shell previously had no way to receive that verified identity before
the temporary session was accepted. Passing the signature or private-key
material to Kotlin would expand the trust boundary and would not provide a
bounded retry or lifecycle policy.

## Decision

Add an opt-in proof-required listener mode to `frameark-ffi`:

1. Rust performs the challenge/signature verification.
2. Rust copies only a request id, sender device id, and 32-byte public key into
   a process-local queue capped at 16 entries.
3. Android polls a fixed, bounded `FAPR` envelope and displays/enqueues the
   copied key through `VerifiedPeerApprovalQueue`.
4. Android returns an explicit request-id approval or rejection through JNI.
5. Rust waits at most 30 seconds and admits the temporary FANP session only
   after approval. Stop/restart clears all pending requests.

PIN-only pairing remains the default, preserving compatibility with senders
that do not implement the optional proof handshake.

## Consequences

Rust remains the authority for proof verification and session admission, while
Android owns presentation and the user decision. The event contains no proof
signature, address, pairing code, URL, or private key. The bridge is a real
approval seam, not automatic persistent trust: enrollment, revocation,
reconnect policy, hardware Keystore signing, and physical-device evidence
remain separate M2 work.

## Evidence

`proof_enabled_network_runtime_surfaces_verified_event_before_acceptance` runs
a pinned QUIC/TLS loopback with a real Ed25519 client: no session is accepted
until the queued request id is approved. `FrameArkNativeTest` validates the
bounded envelope and fail-closed decision handling; Android lint, unit tests,
and debug APK assembly pass.
