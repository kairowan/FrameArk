# FANP transport compatibility

## Current status

**Experimental (M1/M2 foundation, loopback/local-network test scope).** The
Rust `frameark-transport` crate has deterministic control framing, pinned
QUIC/TLS pairing, capability negotiation, bounded FAM1 media framing, and a
sender-owned unidirectional media stream. The Android repository now contains
MediaCodec/AudioTrack lifecycle adapters, but no Android, desktop, or
third-party sender has yet been included in a compatibility matrix.

## Verified behavior

| Scenario | Result | Evidence |
|---|---|---|
| Loopback server with pinned certificate and correct six-digit code | Pass | `pinned_quic_pairing_completes_a_temporary_session` |
| Incorrect code | Pass; client and server report `PairingRejected` | `incorrect_code_is_rejected_without_persisting_trust` |
| Capability offer intersection | Pass; common capabilities are selected and exposed to core session mapping | `capability_intersection_advances_the_shared_core_session` |
| No common capability | Pass; both peers report a negotiation rejection | `no_common_capability_is_rejected` |
| Malformed/oversized frame or capability payload | Parser rejects before payload interpretation | `frame_header_rejects_bad_magic_versions_and_lengths`, `malformed_and_duplicate_entries_are_rejected` |
| Native Offer → Start → Status → Stop lifecycle | Pass on a fake backend; prepare/start/reset and core states are asserted | `frameark-native::native_control_reaches_prepare_start_status_and_stop` |
| FAM1 video/audio media-frame round trip | Pass with bounded exact-boundary parser tests | `frameark-native::media_wire::video_round_trip_preserves_timestamps_and_keyframe`, `audio_round_trip_and_malformed_inputs_are_bounded` |
| Multiple frames on one QUIC media stream | Pass on loopback with explicit FIN and frame-size limits | `frameark-transport::media::sends_multiple_bounded_frames_until_fin` |
| Native packet-to-renderer lifecycle | Pass with fake video/audio renderers, track checks, and partial-prepare cleanup | `frameark-native::media_session` tests |
| Rust QUIC media receiver orchestration | Pass on a real pinned-QUIC loopback: FAM1 frames are decoded, routed to both renderers, counted, and reset on FIN | `quic_media_stream_reaches_renderers_and_cleans_up` |
| Android renderer configuration | Pass for JVM config validation and Android lint/build; hardware behavior unverified | `MediaTrackConfigTest`, Android `lint test assembleDebug` |
| Fixture sender CLI validation | Pass; bounded fixture file, frame-count, interval, pairing-code, and required-input checks | `frameark-cli` unit tests |

## Explicit limits

Control-stream tests additionally cover response delivery before close,
deadlines, dropped request cleanup, trailing-byte rejection, and redacted PIN
debug formatting. These run over real QUIC loopback, not a mocked transport.

- No persistent trust or device identity is established.
- No end-to-end network receiver connects the Rust stream to Android
  MediaCodec/AudioTrack; the Rust bridge is covered only with fake renderers,
  and the Android adapters remain platform lifecycle components.
- No input, clipboard, file-transfer, management stream, congestion-control
  policy, or datagram media path is implemented.
- The fixture CLI accepts real H.264/Opus/AAC access-unit files but the
  repository does not ship a licensed codec sample or decoder matrix yet; no
  1080p playback evidence exists.
- No internet relay, NAT traversal, Cast certification, Miracast guarantee, or
  DRM/HDCP behavior is claimed.
- The self-signed certificate is generated per server process and must be
  delivered and pinned by an in-scope caller.

Before this entry can become Beta, add Android and desktop loopback tests,
network interruption/retry coverage, and a named receiver/sender matrix.
