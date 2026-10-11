# frameark-ffi

`frameark-ffi` is the small JNI boundary used by the Android Receiver. It
exports the versioned ABI check plus a bounded start/stop/running lifecycle
bridge, an ephemeral FANP QUIC listener with best-effort mDNS publication, and an encoded media-frame queue
backed by a Rust `frameark-core::Session` protected by a process-local mutex.

The listener accepts bounded temporary FANP pairings and drives the existing
Rust control/media receiver into the JNI queue. It exposes only its ephemeral
port, pairing code, and pinned certificate through explicit JNI getters. The
listener publishes `_frameark._udp.local.` metadata without the pairing code;
publication is withdrawn when the lifecycle stops and remains optional when a
platform mDNS backend is unavailable. The
bridge still does not own Android `Surface` objects, decode media, or persist
trust. The Kotlin service owns Android lifecycle and notifications; Rust
remains the owner of protocol/session state. Native video and audio submissions
are bounded to 4 MiB each and 16 queued frames, return explicit backpressure,
and are polled as versioned `FAMF` envelopes. After FANP negotiation, the
bridge exposes the latest video and audio track parameters as one-shot,
versioned `FAMC` configuration envelopes before the corresponding media
frames. Kotlin validates the fixed-size envelope and forwards the bounded
configuration to platform-owned decoder/audio sinks; it does not treat the
bridge as a hardware-decoder guarantee.

`FAMC` v1 uses an eight-byte header (`FAMC`, version, media kind, codec, and a
reserved byte). Video adds big-endian width, height, frame-rate numerator, and
denominator (24 bytes total); audio adds sample rate and channel count (16
bytes total). Unknown codec values remain explicit `UNKNOWN` values, while
dimensions, rates, channels, and envelope lengths are bounded at the Kotlin
boundary. A new receiver lifecycle clears pending configurations together with
the media queue.

The test suite includes a real loopback QUIC/TLS integration path covering
temporary pairing, capability negotiation, Offer → Start, one H.264 frame,
one Opus frame, media-stream FIN, Stop, and delivery into the JNI queue.

The native functions return redaction-safe integer status codes and never log
JNI arguments. The shared state is process-local. Stop clears media/session
state, while the installed public identity remains available for a restart.

Before `nativeStartReceiver`, Android must pass the Keystore alias and the
lowercase colon-separated SHA-256 fingerprint of its public certificate through
`nativeSetDeviceIdentity`. Rust validates both values, derives the session/device
ID as `sha256-` plus the 64 fingerprint hexadecimal characters, and rejects
startup with `-5` when no identity is
installed. Only the public fingerprint crosses JNI; private key operations stay
inside Android Keystore. The alias is a local key label, not a unique device ID.
Reinstalling the same identity is idempotent; changing it while a session is
active is rejected with `-2`. The validated alias is retained only as
redaction-safe local metadata for future trust records. Invalid input returns
`-3` without changing state.

The Kotlin shell and native library must be packaged together: startup now
requires the identity setter even though the additive JNI methods leave the
core ABI version unchanged. This is local identity plumbing, not proof of key
possession, persistent peer trust, or binding of the Keystore key to the
ephemeral TLS certificate; temporary certificate pinning and PIN pairing remain
the transport authentication mechanism.
