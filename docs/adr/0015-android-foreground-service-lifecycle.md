# ADR 0015: Android receiver foreground-service lifecycle

- Status: Accepted
- Date: 2026-10-10
- Scope: M2 Android Receiver lifecycle

## Decision

The Android Receiver exposes an explicit, non-exported
`FrameArkReceiverService`. It creates a low-importance notification channel,
enters a media-playback foreground service on Start, and releases its
foreground notification and stops itself on Stop. The activity exposes only
start/stop controls; Rust remains the owner of discovery, pairing, session
state, and media policy.

The manifest declares the Android 14 foreground-service baseline permissions
and `mediaPlayback` service type. Missing intents normalize to Start for
restart safety, while the Stop action is terminal and non-sticky. The service
does not yet open the native FANP session or claim hardware playback.

## Verification and limits

JVM tests cover action normalization and terminal Stop semantics. Android lint,
unit tests, and debug assembly validate the manifest and service integration.
Real-device notification, process-reclaim, boot-start, and one-hour playback
tests remain required for the M2 exit gate.
