# ADR 0047: Android receiver session coordinator

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver lifecycle

## Decision

Keep FANP session state and media ownership in Rust, but centralize the
Android foreground-service boundary in `ReceiverSessionCoordinator`. The
coordinator sequences the Rust JNI lifecycle and the bounded Android polling
loop, reports only redacted lifecycle states, and performs native cleanup when
the platform consumer fails. It is idempotent for repeated start, stop, and
close calls.

The coordinator accepts a small `PlaybackController` interface so JVM tests
can exercise failure and cleanup paths without a device, `Surface`, or
`MediaCodec`. The production implementation remains `MediaPlaybackLoop`.

## Consequences

- The service no longer carries a second lifecycle state machine or duplicates
  native start/stop cleanup.
- A polling/consumer failure becomes an explicit terminal failure and releases
  the Rust session instead of leaving a foreground service apparently active.
- This does not claim Android network attachment, hardware playback, or a
  physical-device compatibility result; those remain M2 follow-up evidence.

## Verification

`ReceiverSessionCoordinatorTest` covers idempotent startup, playback-loop
startup rejection, repeated stop/close cleanup, and the redacted state
transitions. The existing Android lint, JVM test, and debug assemble checks
remain required before merge.
