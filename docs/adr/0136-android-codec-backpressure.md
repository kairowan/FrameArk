# ADR 0136: Bounded Android codec input retry and nonblocking display polling

Status: Accepted

## Faults and scope

The Native playback investigation reproduced two independent Android boundary
faults: `dequeueInputBuffer` returning `INFO_TRY_AGAIN_LATER` was treated as a
fatal decoder failure, and UI/TextureView status reads waited on the same
monitor as synchronous codec work. The latter can stall composition while the
platform output is busy. It is not proof that every observed native-code stall
has the same cause. Full loopback playback has also failed with queue exhaustion;
that remaining failure is not declared fixed by these corrections.

Android's [MediaCodec input contract](https://developer.android.com/reference/android/media/MediaCodec#dequeueInputBuffer(long))
allows a negative result when no input slot is available. A zero timeout requests
immediate polling. [TextureView rendering](https://source.android.com/docs/core/graphics/arch-tv)
depends on UI composition; reading playback information must not wait for the
decoder to finish a platform call.

## Decision

- Audio/video renderers drain output, poll input with timeout zero, and return
  `false` only for `INFO_TRY_AGAIN_LATER`. They never copy or queue the access unit
  on that path. Unexpected indices and actual decoder errors still fail.
- The Android pump retains exactly one rejected event, including its original
  payload/timestamp. Later frames, reconfiguration and ordered reset cannot
  overtake it. Each tick still drains output; there is no spin/sleep in the
  production pump and no new queue of encoded frames.
- A monotonic, absolute 500 ms budget bounds retry of that event; attempts do
  not renew it. Exhaustion follows existing fatal cleanup. Explicit Stop clears
  the retained event immediately and resets output. This is an OS input retry
  budget, not a second media presentation clock. It cannot preempt a stalled
  native platform call.
- Decoder/lifecycle mutations remain serialized. Display size (one immutable
  width/height pair), counters and redacted failure metadata are published with
  volatile/atomic visibility. Diagnostic/display queries take no codec monitor;
  their independent counters are approximate observations, not a transactional
  session snapshot. Gain mutations and resource teardown are not made asynchronous
  by this change.
- Instrumentation preserves the primary failure and attempts every cleanup step;
  subsequent teardown failures are attached instead of hiding the playback fault.
  Temporary thread sampling/debugger instrumentation is not shipped.

Rust still owns protocol/session state, FAPE order, pacing and the unchanged
128-event / 16 MiB / five-second timestamp-admission limits. The Kotlin pending
event is outside that queue and bounded by the existing 4 MiB FAMF payload limit.
No wire, JNI ABI, trust, permissions or dependencies change. Rebuild the APK;
normal ARM64 packages must still exclude the opt-in probe.

## Verification and limits

Four JVM regressions cover exact-event retention, ordering, absolute expiry,
fresh per-event budgets, Stop/restart and non-retryable exceptions. Android
regressions first reproduced fatal temporary audio input and blocked display
reads, then verify real AAC/H.264 recovery and codec-owner-independent polling.
The existing real QUIC decode/output/gain/release checks keep their original
frame/progress/deadline thresholds. See the complete
[investigation record](../evidence/2026-10-17-codec-backpressure/README.md), including
failures; successful repetitions alone do not close stability acceptance.

Native playback remains Experimental. Startup/runtime platform stalls, complete final
PCM drain, physical 1080p30/60, one-hour operation and measured A/V sync remain
separate work. No M1/M2 or M0–M5 exit gate is closed. Rollback is an ordinary PR
reverting this Android-only unit; there is no data migration.
