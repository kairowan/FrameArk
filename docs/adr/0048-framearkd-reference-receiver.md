# ADR 0048: framearkd reference receiver

- Status: Accepted
- Date: 2026-10-11
- Scope: M1 Native vertical closure and protocol lab

## Decision

Add `framearkd` as a small executable reference receiver around the existing
`PairingServer` and `NativeMediaControlReceiver`. It exposes bounded command
line configuration for the bind address, server name, temporary pairing code,
certificate output, timeout, and one-shot behavior. Each accepted connection
uses the shared Rust session state machine and a counting video/audio renderer
factory, then emits only session/media counters and cleanup status.

This executable is intentionally a fixture/reference service. It does not
decode media, own a display or audio device, persist trust, or add a second
protocol state machine. A future desktop or Android receiver must provide
platform renderers behind the same `MediaRendererFactory` boundary.

## Security and limits

- Pairing is temporary and uses the existing pinned certificate and six-digit
  code; no long-term identity or approval is created.
- Certificate output contains public DER only. Pairing codes are printed for
  an operator-controlled local lab and must not be logged in production.
- Every control/media operation keeps the existing bounded timeout and FAM1
  limits; rejected sessions are discarded and backend/renderer cleanup runs.

## Verification

`cargo test -p framearkd` validates bounded option parsing. The existing
`frameark-native` loopback integration test covers the same executable
orchestration path with real pinned QUIC, control messages, FAM1 media, and
renderer cleanup. Manual fixture interoperability remains Experimental until a
named sender/device matrix is recorded.
