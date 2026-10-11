# ADR 0063: AirPlay mirror bounded RTSP runtime

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring TCP boundary

## Decision

Extend `MirrorRtspTcpServer` with a caller-owned `serve_for` primitive. The
listener is temporarily non-blocking, accepts no more than the explicit
request budget, applies a maximum 60-second wall-clock window, bounds each
accepted connection's read/write timeout by the remaining window, isolates
malformed requests, and restores blocking mode before returning.

The scheduler only feeds the existing mirror RTSP state machine. It does not
open RTP sockets, implement Apple pairing/FairPlay, decrypt media, or claim
concurrent production throughput.

## Verification

AirPlay tests cover invalid duration/request budgets and a real loopback
OPTIONS request through the bounded runtime helper. Apple-device and physical
decoder evidence remain required for any stronger compatibility label.
