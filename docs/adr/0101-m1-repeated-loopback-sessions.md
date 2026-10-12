# ADR 0101: repeat the Native loopback session as an M1 stability gate

- Status: Accepted
- Scope: M1 Native vertical-loopback evidence

## Context

The existing FANP integration test proved one Offer → Start → media → Stop
sequence. A single successful run could still hide stale global queue/config
state or a listener teardown race that only appears when the next session
starts.

## Decision

Add a bounded `frameark-ffi` test that executes three consecutive pinned
QUIC/TLS sessions. Each cycle sends one H.264 video frame and one Opus audio
packet, finishes the media stream, stops the session, and asserts that the
receiver observed both negotiated configurations and media kinds. The test
then clears the process-local receiver state before starting the next listener.

## Consequences

- M1 has repeatable local evidence for short-session teardown and restart.
- The test remains deterministic and does not retain media fixtures or
  credentials.
- This is not hardware-decoder, 1080p30, physical-LAN, long-run, or named
  sender/receiver interoperability evidence; those remain open exit gates.
