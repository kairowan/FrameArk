# ADR 0119: Ordered Native playback through Android codecs

Status: Accepted

## Decision

Connect the real Android FANP listener to MediaCodec and AudioTrack, replacing
the service's no-op encoded-frame consumers. Rust retains negotiation, track
ordering, the shared timestamp clock and queue bounds. Kotlin owns TextureView,
Surface, codec input/output, PCM playback and Android lifecycle only.

The experimental Rust `frameark-api::{VideoFrame, AudioFrame}` now carries
`time_base` alongside raw timestamp ticks. Source callers must initialize the
new field; use `TimeBase::new(1, 90000)` for a 90 kHz clock, not its inverse.
`TimeBase::to_microseconds` uses checked-range i128 arithmetic and rejects a
zero numerator or results outside i64. No FANP wire or C struct layout changes.
Rebuild JNI and APK together. This is a pre-1.0 Rust source API migration.

## JNI event contract

`nativePollPlaybackEvent` returns null when no event is ready, otherwise:

| Bytes | Meaning |
|---|---|
| 0–3 | ASCII `FAPE` |
| 4 | Version 1 |
| 5 | 1 video config, 2 audio config, 3 video frame, 4 audio frame, 5 reset |
| 6 onward | Existing `FAMC` config / `FAMF` frame body, or empty for reset |

Only inside FAPE, FAMF's signed big-endian timestamp is **microseconds**.
The legacy direct-submit/poll FAMF development seam remains separate and keeps
its caller-provided ticks. The network service never mixes these two queues.
There are at most 128 events / 16 MiB of queued encoded bytes; overflow rejects
the media session. Config cannot overtake frames. The first frame establishes
a monotonic shared audio/video presentation clock; future frames wait in Rust.
This is basic pacing, not hardware audio-clock drift correction or a measured
lip-sync guarantee. The sender must interleave both tracks on one timeline.
Admission rejects frames more than five seconds ahead of the first-arrival
clock, so a malicious timestamp cannot retain outputs indefinitely after Stop.

Prepare discards any old session and inserts reset before configs. Normal
teardown appends reset after media; if a full queue cannot accept reset, it
discards stale events and preserves reset. Listener stop cancels the consumer,
clears queued state, joins the producer and clears again. Kotlin serializes a
complete dispatch tick against stop, and periodically drains codecs even when
no new JNI frames arrive. Output failure stops the receiver; it is not hidden
as successful playback. Decoder messages/payloads are not logged.

## Supported subset and lifecycle

- H.264 access units must carry Annex-B SPS/PPS and a decodable keyframe at the
  beginning. No arbitrary container demux, HEVC or dynamic reconfiguration yet.
- AAC means raw AAC-LC access units, standard sample rate, mono/stereo. ASC is
  derived from the negotiated rate/channels. ADTS, HE-AAC and arbitrary file
  playback are not this contract.
- Opus means 48 kHz mapping-family-0 mono/stereo raw packets, no pre-skip/gain.
  Kotlin supplies OpusHead and Android's delay/seek-preroll CSD buffers. An
  arbitrary Ogg Opus stream needs a separate demux/config contract.
- PCM output is 16-bit. Codec input is bounded by the platform buffer, each
  drain by 16 outputs, decoded audio by 256 KiB per buffer. Non-blocking audio
  writes retain partial/zero writes in a 64-buffer / 256 KiB owned PCM queue;
  each drain performs at most 16 writes. Negative writes or capacity exhaustion
  fail explicitly. Stop discards queued PCM rather than hanging or playing
  stale audio after release.
- Playback currently requires the foreground Activity's valid Surface. Surface
  destruction stops the service and releases both decoders/audio output.
  Rotation/background continuation is not claimed.

Initial/default-network capability notifications no longer restart the active
listener. A changed/lost default Network identity uses the existing bounded
recovery operation; same-network address changes need explicit restart for now.

The codec configuration follows Android's official
[MediaCodec codec-specific data contract](https://developer.android.com/reference/android/media/MediaCodec).
JNI uses copies in this first measured integration; it is not the final
zero-copy hot path. Queue bytes do not include codec/Java/transient copies.

## Evidence and rollback

Rust tests exercise clock conversion, ordering, byte/event caps, reset and real
QUIC delivery; JVM tests cover malformed FAPE, dispatch order, empty-queue drain
and in-flight-stop synchronization. The opt-in `android-playback-probe` Cargo
feature supplies a loopback sender only for instrumentation; normal builds do
not export its JNI symbols. The test generates valid media using Android's
encoders, connects through pinned QUIC, and checks frame-rendered callbacks and
decoded PCM writes. See the receiver README for reproduction and limitations.
Roll back the paired APK/JNI together; this increment changes no trust storage.
Physical 1080p30/60, one-hour stability, sync, loss/recovery and Apple device
compatibility remain separate PLAN acceptance gates.
