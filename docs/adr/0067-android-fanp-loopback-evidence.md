# ADR 0067: Android FANP loopback vertical evidence

- Status: Accepted
- Date: 2026-10-11
- Scope: M1/M2 Android Rust/JNI integration boundary

## Decision

Keep a real loopback integration test in `frameark-ffi` alongside the unit
tests. The test starts the same `NetworkRuntime` used by the JNI lifecycle,
pins its ephemeral certificate, completes temporary QUIC/TLS pairing and
capability negotiation, sends one H.264 video access unit and one Opus audio
access unit through the native FAM1 media stream, finishes the stream, sends
Stop, and verifies both bounded `FAMF` envelopes reach the JNI queue.

The test uses loopback-only binding and a process-local receiver session under
`cfg(test)`; production still binds all interfaces and requires Android
identity/lifecycle gates. It proves protocol/session/media plumbing, not
hardware decoding, display rendering, audio output, network recovery, or a
physical Android device.

## Verification

`cargo test -p frameark-ffi --offline` covers the six FFI tests, including the
full loopback path. The Android and workspace CI gates remain required before
merging changes that alter this boundary.
