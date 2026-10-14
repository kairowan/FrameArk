# ADR 0127: Receiver-local volume and mute

- Status: Accepted, Experimental
- Date: 2026-10-14

## Decision

Implement M2's local audio controls in the Android OS-output owner. A bounded
`PlaybackAudioLevel` contains integer percent (0..100) and a separate mute flag.
Fresh installs default to 50%, unmuted; existing installs without these keys use
the same default. Gain is linear percent / 100, or zero when muted, not a dB
or perceived-loudness scale. Setting zero and selecting mute are distinct.
Changing percent never implicitly unmutes. No system-wide volume is changed.

`ReceiverPlaybackOutput` serializes UI changes with decode/drain/reset. Both
Activity and Service load app-private preferences before playback, once per
process; subsequent initialization cannot overwrite a live choice. Decoder,
surface and session resets preserve that choice. The PCM adapter sets gain
**before** `AudioTrack.play()` and checks the platform return code. Muting does
not pause decoding or queue old audio: the existing bounded nonblocking write
path continues to consume PCM at zero gain.

`PcmOutputTrack` is an internal Kotlin test seam, replacing the pre-alpha
AudioTrack factory type. The production implementation still owns one real
AudioTrack. No native ABI, FANP message, Rust session policy, dependency,
permission, remote input, or new audio route is introduced. Sender-side remote
volume and DLNA/RAOP volume mapping are separate protocol work.

## UI, persistence and failure

The foreground Receiver page exposes a slider, ±10 buttons and mute checkbox.
The slider supports D-pad ±5 increments; all controls use native focus semantics
and obscured-touch filtering. English/Chinese labels distinguish app-local
volume from system volume. Chinese Receiver resources are completed alongside
the new localized panel; diagnostic counters remain English.

Two non-sensitive values live in app-private `playback-audio` preferences, not
the trust store. Backup is already disabled for the app. Button/key adjustments
persist immediately; touch dragging applies live gain and persists on release.
An interrupted adjustment before release is not promised durable. Synchronous
`commit()` is deliberately used for a write receipt, not on every drag event;
the page reports a failed write while keeping the live value. Cold-start read
failure, wrong types and out-of-range data select zero and muted. Clearing app
data removes the preference and restores the fresh-install default.

Live gain failure closes the output, remembers zero gain, forces local mute,
resets codecs and requests service stop. UI shows failure and attempts to save
the muted intent; that best-effort save is not a guarantee after storage failure.
Restart is explicit, and does not automatically unmute. Diagnostics retain only
a static code location, never platform exception messages, media or peer data.
This is playback control, not a privacy barrier against other apps/system audio.

## Verification and limits

JVM fake-track tests verify gain-before-play, bounds, persistent gain across
reset, continued muted PCM consumption and setup/live/release failure cleanup.
Isolated API 34 tests exercise actual AudioTrack return values, D-pad behavior,
preference corruption and UI failure reporting. The test-only pinned-QUIC native
probe changes controls while real H.264/AAC decode/output continues.
See [evidence](../evidence/2026-10-14-audio-controls/README.md).

These are not physical loudness/mute latency, Bluetooth/HDMI routing, Android TV,
lip-sync, long-run or M2 acceptance results. The [owner test guide](../testing/ANDROID_AUDIO_CONTROLS.md)
requires owner-operated hardware and unprotected, non-private test audio.

## Primary reference

[Android AudioTrack.setVolume](https://developer.android.com/reference/android/media/AudioTrack#setVolume(float))
defines this per-track linear gain and status return. The application checks the
status rather than claiming success solely because the slider changed.
