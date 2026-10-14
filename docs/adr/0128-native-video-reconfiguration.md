# ADR 0128: Ordered Native video reconfiguration

- Status: Accepted, Experimental
- Date: 2026-10-14

## Decision

Extend the pre-alpha FANP control profile with explicit `OfferReconfigurable`
(opcode 5) and `UpdateVideo` (opcode 6). The former confirms support before
capture begins; it does not silently downgrade on an older peer. Existing
Offer/Start/single-media-stream/Stop behavior remains available unchanged.
New screen-capture senders require a matching receiver; reference senders retain
the fixed profile. TLS/PIN/proof and capability authentication are unchanged.

Within one authenticated connection, finish the current ordered FAM1 stream,
then send a sequenced UpdateVideo, await its exact configuration acknowledgment,
and open the next media stream. This creates an unambiguous boundary without
cross-stream frame/configuration races or cancelling a partially read control
message. FAM1, track IDs, audio configuration, timestamps and sequence counters
continue unchanged. Reconfiguration does not restart pairing, audio, or the
JNI playback clock. Rust owns validation, policy, request bounds, the shared
Streaming → Reconfiguring → Streaming lifecycle and first-new-video-keyframe
requirement. Failure/cancellation is terminal and releases outputs.

Only video dimensions/frame rate can change within the existing H.264 policy.
Audio/codec/track changes are not implicit. Backend support is explicit and
defaults to unsupported. A maximum of 256 video updates per connection limits
decoder rebuilds; the existing per-connection 1024-request ceiling and
bounded I/O/queue limits apply. No concurrent control/remote-input channel,
network reconnect, new crypto, dependency or production key is introduced.

Android applies only OS capture/codec/render changes. Resize the one existing
VirtualDisplay and replace its encoder surface; never reuse the OS grant to
create a second display. API 34 captured-region callbacks supply actual window
dimensions; older supported capture APIs use default-display size changes.
Coalesce one pending 16..16384 OS size after 250 ms on the encoder worker, retaining
the user's 720p/1080p preset without upscaling. Receiver configuration
events rebuild video only, keep local audio gain/mute, and preserve aspect ratio.
Size/orientation configuration changes retain the foreground playback surface;
other Activity destruction still releases playback rather than retaining a
hidden capture/playback grant.

## Compatibility and evidence

The wire extension is explicitly requested, not falsely advertised through
the old broad capability bitset. Older receivers reject opcode 5 before media;
there is no retry, automatic PIN/proof downgrade or reconnection fallback.
Additive JNI methods require the matching APK/native library pair. Physical
decoder and rotation performance remain separately verifiable; a passing
parser/mock test cannot establish dynamic Android output.

See [verification evidence](../evidence/2026-10-14-video-reconfiguration/README.md)
for real two-emulator OS capture, QUIC/JNI/codec rotation and negative tests.
The additive pre-alpha Rust API adds default-unsupported backend methods and
`NativeMediaSessionReport.video_updates`; exhaustive external struct literals
must account for that field. No stable API/ABI compatibility promise is made.

## Primary references

- [MediaProjection sizing and consent](https://developer.android.com/media/grow/media-projection)
- [MediaProjection resize callback](https://developer.android.com/reference/android/media/projection/MediaProjection.Callback#onCapturedContentResize(int,%20int))
- [VirtualDisplay.resize](https://developer.android.com/reference/android/hardware/display/VirtualDisplay#resize(int,%20int,%20int))
- [Android configuration continuity](https://developer.android.com/guide/topics/resources/runtime-changes)
