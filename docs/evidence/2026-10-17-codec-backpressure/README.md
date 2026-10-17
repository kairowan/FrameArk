# Android Native codec-boundary investigation — 2026-10-17

Scope: focused branch from main `9db467b01cef96a8ab50a4731511c8aa5d3f8e33`.
Only agent-owned API 34 x86_64 AVD `frameark_proof_ui` (`emulator-5582`) was
operated. No owner's phone/TV was accessed. Both APKs use the same main-built
opt-in probe JNI; the fix changes Kotlin, not Rust queue/transport behavior.
See [decision and bounds](../../adr/0136-android-codec-backpressure.md).

## Deterministic failures before the correction

- Injecting three temporary `INFO_TRY_AGAIN_LATER` results into a real configured
  AAC decoder fails on the first attempt with `audio decoder backpressure`
  (one test, one error, 0.781 s). Only the input-index seam was added to reproduce
  the old fatal behavior; the codec and encoded access unit are real.
- A separate thread holds the codec-owner monitor while another reads display
  dimensions/counters/diagnostics. The old synchronized reads exceed the 500 ms
  test bound (one test, one timeout, 0.549 s). Cleanup releases/joins both workers.

## Additional investigation, including failures

Input retry alone did not remove all loopback failures. Temporary bounded thread
sampling observed the playback worker inside platform dequeue/queue/release or
AudioTrack setup while the UI waited in synchronized diagnostic/display reads.
One sampling helper initially raced thread creation; that helper was corrected,
then removed entirely. Sampling/debugger runs are diagnostic interventions, not
performance acceptance evidence. A debugger run delivered 90 video frames but
only 80 completed PCM buffers and failed the unchanged 100-buffer assertion.

After both corrections, an initial four-test run on the restarted AVD still
failed loopback with `playback.queue_unavailable` (2 submitted / 1 rendered video,
0 PCM); the three focused regressions passed. No queue limit or assertion was
relaxed. Another emulator session ended before a suite completed, and a restart
attempt lacked the target APK; these incomplete/setup runs are not passes.

Six consecutive subsequent loopback-only runs, without a sampler/debugger, passed:

| Run | Duration (s) | Video submitted/rendered | Completed PCM buffers |
|---|---:|---|---:|
| 1 | 5.936 | 90/90 | 108 |
| 2 | 5.931 | 90/90 | 128 |
| 3 | 5.823 | 90/90 | 129 |
| 4 | 5.576 | 90/90 | 126 |
| 5 | 5.774 | 90/90 | 126 |
| 6 | 6.252 | 90/90 | 133 |

These repetitions are not a replacement for the failing result above. They
verify ordinary playback, gain/mute changes, output reset and listener teardown
under those runs; they do not prove startup stability or audible presentation.

Three following complete four-test suites produced: 4/4 pass (6.859 s, 90/90
video, 133 PCM), 4/4 pass (7.365 s, 90/90 video, 135 PCM), then 3/4 pass
(8.146 s). The last loopback failed mid-stream with `playback.queue_unavailable`
at 38 submitted / 37 rendered video and 55 PCM. All three focused regressions
passed in every suite. Thus the remaining failure is not limited to startup.
`ReceiverNetworkContinuityTest` passed its original listener-continuity check
(one test, 0.838 s).

Local JVM checks: 114 tests, zero failures/errors (103 Receiver, 11 Sender),
including four new pump cases. Receiver debug lint: zero errors, 15 existing
warnings. Python repository-tool regressions: 23 passed. Repository policy and
whitespace checks passed. These checks do not execute physical-device acceptance.

Full Rust workspace/all-features tests passed: 305 passed, zero failed; the one
operator-authorized physical multicast/private-LAN test remains ignored. DLNA's
nine-case instrumentation passed (29.239 s), including pixels, captions, media
replacement and teardown. Audio controls initially crashed on a UI-test
`requestFocus()` assertion (not a decoder exception). The isolated D-pad case then
passed on both the before-fix APK (2.892 s) and this APK (2.743 s); the complete
four-case audio-control suite passed on repetition (3.297 s). This timing-sensitive
fixture observation is retained, not described as an uninterrupted all-tests pass.

## Reproduction and remaining work

Use the receiver README's explicit probe-JNI/Gradle opt-in, then run
`dev.frameark.receiver.NativePlaybackTest` with the Android instrumentation runner.
The class includes both codec input regressions, the monitor-contention regression
and original pinned-QUIC video/audio playback. JVM `PlaybackEventTest` adds four
retry/deadline/ordering tests without platform timing.

The startup/runtime platform stall and final PCM draining need further investigation;
no claim is made that all earlier queue/audio/listener failures share one cause
or that all Native bugs are resolved. Keep the previous preview's instability
warning. Physical-device, 1080p performance, long-duration, recovery and A/V sync
gates remain open. No production secrets, signed release, tag, or device-matrix
claim is introduced.
