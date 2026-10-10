# frameark-ffi

`frameark-ffi` is the small JNI boundary used by the Android Receiver. It
exports the versioned ABI check plus a bounded start/stop/running lifecycle
bridge backed by a Rust `frameark-core::Session` protected by a process-local
mutex.

The bridge deliberately does not open sockets, own Android `Surface` objects,
decode media, or persist trust. The Kotlin service owns Android lifecycle and
notifications; Rust remains the owner of protocol/session state. Discovery,
QUIC media delivery, and renderer callbacks require separate increments.

The native functions return redaction-safe integer status codes and never log
JNI arguments. The shared state is process-local and is reset on stop.
