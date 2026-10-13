# ADR 0117: Foreground Android proof approval

Status: Accepted

## Decision

The receiver Activity displays one Rust-verified FANP proof request at a time.
It shows the claimed device ID and SHA-256 of the raw Ed25519 public key, not an
unverified friendly name. A main-thread coordinator projects Rust's pending
query onto the UI and sends only an explicit allow/deny decision for the
displayed request ID. It does not own protocol deadlines or persistent trust.

Polling runs every 250ms only while resumed. Pausing or explicitly stopping the
receiver rejects the displayed request and removes its UI state. Rust owns the
30-second deadline, duplicate-decision rejection, cancellation, and queue bounds
from ADR 0116. Every decision is checked again by Rust, including a click racing
expiry. Kotlin keeps no key bytes after constructing the fingerprint and does
not restore a prompt after Activity recreation. Both decision buttons reject
obscured touches. Proof mode can only be selected or disabled while stopped.

## Validation and limitations

JVM tests cover display without automatic approval/enrollment, exact request-ID
binding, duplicate/stale clicks, expiry before poll or during a click, explicit
rejection, pause dismissal and unavailable native libraries. Real QUIC delayed
approval and cleanup are tested independently in `frameark-ffi`.

The [Android 14 emulator smoke record](../evidence/2026-10-13-proof-ui/README.md)
confirms bundled JNI loading, disabled idle decisions, proof selection and
rejection of a running-mode change; it does not exercise an incoming proof.

This is not a full sender UI, persistent trust enrollment, live trust revocation
or physical-device playback evidence. Bundled Kotlin and JNI must be rebuilt
together for `nativePeerProofPending`. PIN-only remains the process default;
the app documents the need for a proof-capable sender when proof mode is used.
