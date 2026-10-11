# ADR 0058: Android FANP endpoint lifecycle coordinator

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver service/UI boundary

## Decision

Add a Kotlin `ReceiverNetworkCoordinator` that snapshots the ephemeral FANP
port, pairing code, and certificate from `FrameArkNative` after Rust startup.
`ReceiverSessionCoordinator` treats a missing or invalid endpoint as a startup
failure and stops the native session before starting the media polling loop.
The foreground service may display only the port and temporary code in its
user-visible notification; certificate bytes remain in memory and are never
logged or persisted.

This keeps protocol state and endpoint ownership in Rust while giving the
Android lifecycle a deterministic, testable pairing affordance. It does not
claim mDNS publication, persistent trust, a decoder/surface callback, or
physical-device playback.

## Verification

JVM tests cover endpoint snapshots, invalid endpoint rejection, material
clearing on stop, and service startup ordering when the endpoint is missing.
Android lint and unit tests remain required CI gates.
