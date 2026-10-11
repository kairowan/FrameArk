# ADR-0081: Add an experimental Android trust settings surface

## Status

Accepted — experimental M2 UI increment.

## Context

The Rust trust registry and encrypted snapshot adapter were available, but no
Android path allowed an operator to explicitly approve or revoke a remembered
peer. Reimplementing rotation or capacity policy in Kotlin would create a
second source of truth.

## Decision

Add `TrustSettingsController` and a minimal `MainActivity` surface with device
ID, fingerprint, and label fields plus approve/revoke actions. The controller
delegates all validation and mutation to `FrameArkNative`, persists through
`AndroidTrustSnapshotStore`, and refuses changes while the receiver is running.
If persistence fails while stopped, it attempts to restore the prior FTR1
snapshot. The UI warning requires values to come from an authenticated future
pairing flow and explicitly says that typed fingerprints are not proof.

This is not a sender-authentication protocol, a peer list, or a pairing UX.
It is an operator-facing experimental control for the existing Rust policy and
is not eligible for a Stable compatibility claim.

## Verification and limits

JVM tests cover approve/revoke persistence and the streaming lockout; Android
unit tests, lint, and a debug APK build pass. Physical device rendering,
pairing-driven approval, sender key-possession proof, and accessibility review
remain open.
