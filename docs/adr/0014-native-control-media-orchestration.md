# ADR 0014: Native control and media orchestration

- Status: Accepted
- Date: 2026-10-10
- Scope: M1 Native vertical closure

## Decision

Add `NativeMediaControlReceiver` as the single-session Rust orchestrator for
the M1 sequence. It validates one Offer against negotiated capabilities and
receiver policy, prepares the platform backend and renderer factory, accepts
Start, opens one sender-owned FAM1 media stream, drains it through
`NativeMediaReceiver`, then accepts Stop and releases all backend and renderer
resources. The returned report combines redaction-safe control lifecycle
events with media frame/byte counters.

The orchestration is intentionally one-session and bounded. It does not add a
second state machine to Android, expose codec internals to protocol code, or
claim persistence, reconnection, congestion control, or hardware playback.
Renderer creation stays behind `MediaRendererFactory`, so Android and desktop
adapters can bind their own surfaces and decoder lifecycles later.

## Verification

The native integration test performs temporary pinned QUIC pairing, sends real
Offer and Start control requests, streams one FAM1 video and audio packet,
finishes the media stream, sends Stop, and verifies the control states, media
counters, backend calls, renderer calls, and cleanup counts.
