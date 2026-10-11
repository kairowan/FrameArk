# ADR-0086: Add an expiring Rust-owned trust approval queue

## Status

Accepted — M2 trust-policy increment.

## Context

The Android surface could approve or revoke a manually entered fingerprint,
but there was no Rust-owned ephemeral state for a transport to present a
pending approval request. Reimplementing request expiry, duplicate handling,
or registry mutation semantics in Kotlin would create a second trust policy.

## Decision

frameark-trust now provides PendingTrustRequests, capped at 16 entries.
Each request contains a bounded device ID, fingerprint, and label, receives a
monotonic lifetime of at most one hour, and is absent from the persistent FTR1
snapshot. Requests are deduplicated by device ID plus fingerprint, can be
listed for a UI, and are removed after explicit rejection or expiry.

Approval delegates to TrustRegistry::trust. If capacity or silent key
rotation rejects the mutation, the pending request remains available; it is
removed only after the registry accepts the mutation. The queue deliberately
does not verify signatures or certificates. A transport adapter must perform
key-possession verification before enqueueing the request and must authorize
the UI that can approve it.

## Verification and limits

Rust tests cover TTL bounds, deterministic IDs, duplicate refresh, expiration,
redacted debug output, explicit rejection, successful approval, and retention
after registry capacity failure. Android/JNI transport integration and
authenticated sender proof remain open work; this increment is not a pairing
protocol or a Stable trust claim.
