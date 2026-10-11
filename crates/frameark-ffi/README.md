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
and are polled as versioned `FAMF` envelopes.

The test suite includes a real loopback QUIC/TLS integration path covering
temporary pairing, capability negotiation, Offer → Start, one H.264 frame,
one Opus frame, media-stream FIN, Stop, and delivery into the JNI queue.

The native functions return redaction-safe integer status codes and never log
JNI arguments. The shared state is process-local and is reset on stop.
