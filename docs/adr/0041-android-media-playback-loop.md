# ADR 0041: Android bounded media playback loop

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver lifecycle

## Decision

Add `MediaPlaybackLoop` as the Android scheduling owner for the existing
`MediaPlaybackPump`. It runs bounded `drainOnce` calls on a single daemon
executor at a validated 1–1000 ms interval, stops idempotently, and reports a
consumer exception to the service before cancelling future ticks. The Android
foreground service starts the loop only after the Rust lifecycle accepts the
session and stops it before native teardown.

The loop does not decode media, own a `Surface`, own an `AudioTrack`, or move
session state into Kotlin. The injected consumers remain the platform codec and
audio boundary; they may apply their own backpressure and configuration.

## Failure and cleanup

Native ABI unavailability prevents executor creation. Stop and close cancel the
scheduled future and release the executor; native session teardown remains
explicitly owned by the service. Consumer failures are terminal for that loop,
avoiding a hot error loop, and are delivered through a redacted lifecycle
callback supplied by the service.

## Evidence and limits

JVM tests cover scheduled video/audio dispatch, unavailable-native startup,
bounded intervals, clean stop, and consumer-failure cancellation. Android
lint/unit/build checks pass, but physical JNI loading, codec output, network
attachment, 1080p60 playback, and long-run device stability remain unverified.
