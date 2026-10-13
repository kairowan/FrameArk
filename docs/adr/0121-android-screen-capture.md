# ADR 0121: explicit Android OS screen and playback capture

- Status: Accepted for Experimental use
- Scope: M2 real source increment, not the M2 exit gate

## Decision and boundary

Extend the separate Sender APK with Android 10+ MediaProjection, surface H.264
encoding and optional AudioPlaybackCapture/AAC-LC. Keep the 30-second synthetic
mode unchanged. Rust still owns pinned TLS/PIN, Offer/Start/media/Stop, sequences,
session timebase conversion, bounded queues and deadlines. No FANP wire changes
or new third-party dependencies. Existing dual-license terms continue to apply.

The new JNI startCapture validates even dimensions 16..1920, at most 1920×1080
pixels, 30/60 fps and explicit audio enablement. Portrait is admitted within the
same area bound. The receiver's common policy still decides actual negotiation.
Reference mode retains 31-second PTS/60-second worker limits; capture permits
PTS up to 3601 seconds, worker lifetime 3660 seconds including setup/teardown.
The OS producer ends at 3600 seconds. These are bounds, not soak evidence.
Payload/queue/backpressure/cancellation limits from ADR 0120 are unchanged;
audio packets are rejected when no audio track was offered.

## Consent, lifecycle and privacy

Inspect/confirm exact pairing ticket and enter PIN, then optionally grant audio
permission, grant notification permission and confirm the OS sharing dialog.
Denial does not fall back to a microphone, silent automatic retry or hidden
capture. Each grant is passed once to a non-exported mediaProjection foreground
service, after consent and startForeground, and used for exactly one virtual
display. Register the projection callback before creating that display. There
is no BOOT_COMPLETED start, reusable token, exported binder or sticky restart.
Audio matches MEDIA/GAME/UNKNOWN usage and respects source/profile policies.

Credentials remain in process memory via a single-use 15-second handoff; Intent
extras contain its opaque ID and OS grant, not ticket/PIN. UI fields disable
autofill/state saving and the Sender activity remains FLAG_SECURE. Background
capture is deliberately allowed only for the consented service. UI and ongoing
notification provide Stop; OS revocation, screen-off and task removal stop it.
Resource order: release virtual display and AudioRecord, stop/release codecs
and input surface, finish the Rust queue/control exchange, stop projection and
foreground service. Force destruction cancels native work. Completed access
units already queued may drain; cancellation does not promise every final frame.
Generation-bound diagnostics prevent a finishing old service overwriting a new
run. Errors expose static source locations/numeric native stage, never exception
messages, credentials or captured payloads. There are no recording files.

## Media and current limitations

Geometry is fit to the initial display without upscaling, even-aligned, with
fixed landscape/portrait canvas for the session. The OS may letterbox changed
content; this is **not dynamic codec/session reconfiguration**. Video PTS uses
the surface monotonic clock relative to capture start. AAC input uses captured
PCM frame position and AudioRecord's monotonic timestamp when available, with
sample-count fallback. Codec priming before time zero is dropped. No measured
lip-sync or long-run drift correction is claimed. Non-silent PCM blocks are a
redacted source diagnostic, not proof of audible receiver output.

Requested FPS is not measured FPS. Surface repeat requests keep static screens
alive; eight seconds without encoded video fails visibly. MediaCodec, projection
and AudioRecord calls are platform-owned and cannot be forcibly bounded safely.
Normal output is complete Annex-B H.264 access units with SPS/PPS at IDR plus raw
AAC-LC. No microphone, DRM/HDCP bypass, automatic quality downgrade, proof-capable
Sender, discovery UI, reconnect or one-hour hardware Beta promise is introduced.

## Verification and primary references

Native config/queue/lifetime tests, JVM bounds/single-use/redaction/stale-generation
tests, normal Android lint/build and opt-in fixture compilation are CI gates.
Runtime tests use disposable emulators and real OS consent, projection, playback
capture, codecs, Rust transport, decoded pixel checks and resource release.
Test-only fixture activity exists only with framearkPlaybackProbe=true; normal
owner APKs must omit that flag and the Rust android-playback-probe feature.

- [Android MediaProjection](https://developer.android.com/media/grow/media-projection)
- [Android playback capture](https://developer.android.com/media/platform/av-capture)
- [Media projection foreground service](https://developer.android.com/develop/background-work/services/fgs/service-types#media-projection)

These document platform requirements, not a claim that every supported Android
device behaves identically. Rollback: use synthetic reference mode, or revert
this increment through a reviewed PR; never relax authorization or TLS checks.
