# ADR 0131: Rust-owned DLNA media source and acknowledged Android player

- Status: Accepted, Experimental
- Date: 2026-10-15

## Outcome and ownership

Connect the existing MediaRenderer control/event service to real Android output.
Rust owns HTTP/SSDP/SOAP/GENA, URL/address policy, revocable media handles, command
generations, feedback filtering and the shared `frameark_core::Session`. Android
owns only interface selection/consent, foreground service, multicast lock, Surface,
MediaDataSource and MediaPlayer calls on one HandlerThread Looper. No production
key, tag, release, protocol certification or third-party source code is introduced.
The FFI adds a local dependency on the already-existing dual-licensed DLNA crate;
the external lockfile package set is unchanged.

The app is opt-in and foreground-only. It cannot run alongside the Native receiver.
Private IPv4 is required; discovery publishes root/UDN/device and three services,
refreshes alive every 120 seconds (300-second cache), and sends byebye on orderly
shutdown. SSDP requires the socket on port 1900; a conflict fails startup. Search
responses are globally rate-limited to ten requests/second. The existing publisher
now emits HOST for NOTIFY and empty EXT for responses, and device XML contains the
MediaRenderer deviceType. The stable UUID v8 UDN derives from the installed public
identity fingerprint, accepting the existing colon-separated format.

## Media, feedback and lifetime

`RemoteMediaSource` probes byte 0, requires exact 206 ranges and known nonzero
length <=64 GiB, then serves one <=64 KiB cached chunk with bounded random reads.
Connections use only supplied validated/pinned addresses, retain the original
Host/SNI and never redo DNS. HTTPS requires explicit roots and certificate checks;
the Android host does not yet supply roots and therefore rejects HTTPS. Redirects,
credentials, compression, chunked/unknown-length sources and ignored Range fail.
MIME/total length and any strong ETag must remain stable. Without a strong ETag,
same-size/type replacement detection is unavailable. One concurrent read is allowed;
close uses an independent socket handle to interrupt reads. The total deadline
also bounds slow trickles and connection attempts; it is not an idle timeout.

The Android host additionally binds each source to its SOAP controller's peer IP.
Literal hosts must match. Hostnames retain their authority but connect to that peer
without DNS; they are not proof of origin ownership. Other hosts are rejected.
This current profile accepts MP4/MP3/WAV MIME types only and is not the full M3 scope.

At most two non-reused opaque source handles permit old/new source replacement.
No URL crosses JNI. Android returns short reads/EOF through MediaDataSource, with
64 KiB maximum allocation/copy per read. This is a bounded-copy baseline, not a
zero-copy or measured-throughput claim. A one-slot command bridge waits at most
five seconds. SetURI is acknowledged only after OS onPrepared; Seek/Stop after
onSeekComplete. Play/Pause/volume acknowledge the OS call, not rendered frames.
Shared Session follows preparation/streaming/draining/closed transitions; there
is no separate Kotlin protocol state machine. DLNA's Authenticating phase does
not add PIN or identity proof to this unauthenticated protocol.

Each runtime consumer and command has a process-unique monotonically allocated
token. An old polling thread cannot steal new runtime commands. Duplicate/stale
acknowledgments and feedback are rejected, including callbacks racing replacement.
Feedback immediately after ACK is retained even before the Rust waiter resumes.
Actual OS position/duration, EOS and asynchronous failure reconcile service state;
progress cannot overwrite an unconsumed terminal observation and failure dominates
EOS. Fatal output errors detach the backend, clear media and revoke handles.

Leaving the activity stops the service. Rust sets stop, closes the bridge/sources,
joins HTTP/SSDP workers and removes the bridge. Kotlin closes sources before OS
release on its Looper. A pending initial source probe is bounded by two seconds;
an in-flight connect may finish only at its deadline. Arbitrary stuck OS codec
work cannot be forcibly cancelled by Rust. There is no background restart.

## Callback scheduling and diagnostics

The TCP server exposes a peer/request filter before resource/SOAP access. The host
accepts private IPv4 controllers, and callbacks only to literal subscriber peer
addresses. This is not same-subnet enforcement or authentication. At most 16 leases
and 32 pending service events plus 32 drained host events are retained. Initial
snapshots preserve the existing response-close/SEQ-0 rule; noninitial events recheck
their live service lease before delivery. One callback is attempted no sooner than
200 ms after the previous attempt, with total 200 ms I/O deadline and no retries.
Request parsing/control response writes also have total deadlines. `serve_for`
bounds admission time; an accepted request gets its full configured deadline,
not the scheduler's remaining milliseconds. Backend time remains separately bounded.
Failed callback
attempts, HTTP requests, output acceptance/failure and source I/O are counted without
media, URL, metadata or credentials. Source-byte counts and video-start notifications
are not frame counts or audible-quality evidence.

## Migration, evidence and remaining gates

Custom backends may implement optional nonblocking `feedback`; their Rust host must
filter stale generations. Call `poll_output` regularly (handle also polls). Existing
non-feedback backends remain valid. HTTP hosts can install `set_request_filter` and
check `event_is_deliverable` after draining; they still own authorization and timing.
New JNI methods require matching native/APK versions; normal builds omit startProbe.

Tests include pinned range/TLS source behavior, cancellation, source changes,
slow-trickle deadlines, real HTTP peer filtering, feedback/EOS/failure, stale consumer
tokens, single-use ACK, timeout/close, UUID formatting and real host start/stop.
Android tests generate their own red H.264 MP4 and PCM WAV, exercise actual TCP/JNI/
MediaPlayer, assert decoded pixels and controls, then replace the source and observe
audio position/EOS. See [evidence](../evidence/2026-10-15-dlna-playback/README.md).

Complete mandatory services/action-argument SCPD, SSDP MX jitter/port-sharing and
multi-interface interoperability, images, subtitles, playlists, wider authorized
media origins, Android TLS trust integration, MP3/other-codec evidence, named
Android/iOS/desktop controller matrix and two-hour playback/Seek remain open. This
is an interoperable building block, not completed M3 or certified DLNA behavior.
Rollback uses a reviewed revert and disables the entry point; no persisted schema
or production-signing migration is required.

## Primary sources

- [Android MediaDataSource](https://developer.android.com/reference/android/media/MediaDataSource)
- [Android MediaPlayer threading and state contract](https://developer.android.com/reference/android/media/MediaPlayer)
- [UPnP Device Architecture 1.0](https://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v1.0-20080424.pdf)
- [AVTransport:1](https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf)
