# ADR 0012: Native media receiver orchestration

- Status: Accepted
- Date: 2026-10-10
- Scope: M1 Native media stream and M2 Android integration boundary

## Decision

`frameark-native` owns the bridge from a transport `MediaReceiver` to a
prepared `MediaSession`. `NativeMediaReceiver` receives bounded length-prefixed
FAM1 records, decodes each record with the native wire parser, routes the
validated packet to the matching platform renderer, and returns only bounded
frame/byte counters. It resets both renderers on clean FIN, peer disconnect,
decode failure, renderer failure, or cancellation through ownership cleanup.

Transport framing remains in `frameark-transport`; FAM1 parsing remains in
`media_wire`; Android and desktop adapters remain responsible for their native
codec and output APIs. The receiver treats the transport's existing
`ConnectionClosed` result as stream termination because the current QUIC
receiver does not distinguish a clean FIN from an abrupt peer close. A future
transport diagnostic can add that distinction without changing the renderer
boundary.

## Verification

The native crate includes a real loopback test that pairs over pinned QUIC/TLS,
sends one video and one audio FAM1 frame on one media stream, verifies both
fake renderers receive exactly one access unit, checks the byte/frame report,
and asserts both renderer reset paths execute.

## Limits

This closes the Rust transport-to-renderer bridge but does not claim an Android
network session, codec decode success, Opus/AAC decoding, 1080p30 playback, or
real-device stability. Those remain M1/M2 work and require platform fixtures
and hardware evidence.
