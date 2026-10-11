# ADR 0064: AirPlay RAOP bounded TCP runtime

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay/RAOP audio TCP boundary

## Decision

Add `RaopRtspTcpServer` as a caller-owned adapter around the bounded RAOP RTSP
state machine. It accepts one request at a time or runs a temporary
non-blocking scheduler with an explicit request budget and a maximum 60-second
wall-clock window. Each accepted connection receives read/write timeouts
bounded by the remaining window, malformed requests are isolated, and the
listener returns to blocking mode before the helper exits.

The adapter owns only the TCP listener and RAOP session state. UDP RTP sockets,
Apple pairing, FairPlay negotiation, encryption/decryption, audio decoding,
and platform output remain separate layers. This keeps the M4 transport
boundary deterministic without implying certified AirPlay compatibility.

## Verification

The AirPlay test suite covers invalid runtime and batch budgets plus a loopback
`OPTIONS` request that traverses the bounded RAOP runtime and returns a valid
RTSP response. Apple-device, pairing, encrypted-media, decoder, and long-run
evidence remain required before a stronger compatibility label is possible.
