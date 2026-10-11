# ADR 0070: Android bounded session recovery seam

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android receiver lifecycle

## Decision

`ReceiverSessionCoordinator` exposes an explicit `recover()` operation for a
transport or network observer. Recovery stops the Android polling loop, closes
the Rust-owned receiver and endpoint view, marks the redacted state as
`Recovering`, and runs the existing one-shot start path exactly once. A failed
restart reports the same bounded failure reasons as initial startup.

Kotlin does not implement a second FANP state machine, infinite retry loop, or
background network scanner. The caller decides when a short interruption is
recoverable and may apply its own user-visible backoff policy.

## Cleanup and safety

Recovery releases playback and network owners before starting a new native
session. Normal stop now releases the optional network coordinator as well as
the playback loop and native lifecycle, so endpoint material cannot outlive
the session. All operations remain serialized by the coordinator lock and are
idempotent at the native boundary.

## Verification and limitation

- JVM tests prove a running session restarts in the order
  `playback stop → native stop → native start → playback start`.
- Tests prove recovery is ignored outside `Running` and does not create hidden
  native work.
- This is a recovery seam, not evidence of automatic Wi-Fi monitoring,
  reconnect success on physical hardware, or M2's one-hour stability exit
  condition.
