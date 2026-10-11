# ADR 0044: atomic AirPlay mirror reconfiguration

- Status: Accepted
- Scope: M5 AirPlay screen-mirroring RTSP state boundary
- Date: 2026-10-11

## Context

Mirror sessions can change coded dimensions, orientation, audio clock, or UDP
control/timing ports while a sender remains connected. The previous state
machine only accepted SETUP from Idle, making a daemon choose between silently
ignoring an update or tearing down a session without a protocol-level reason.

## Decision

`MirrorSession::handle` accepts SETUP from both Idle and Streaming. It parses
and validates the transport and the complete XML/binary plist before mutating
state. A successful update replaces the transport and configuration as one
operation. Idle transitions to Setup; Streaming remains Streaming so the
caller can coordinate decoder/socket changes without a second RECORD handshake.

The protocol layer does not own sockets, decoders, or jitter pipelines. A
platform/daemon integration must apply the new configuration to those resources
and decide whether a keyframe or audio-clock reset is required.

## Consequences

- Validated orientation/dimension/clock updates are observable without losing
  the session state.
- Malformed or unsupported updates cannot partially replace the live setup.
- This is a protocol boundary only; it does not claim Apple-device
  interoperability, encrypted media support, or seamless decoder recovery.
