# ADR 0009: bounded FANP QUIC media stream

- Status: Accepted
- Date: 2026-10-09
- Scope: M1 Native vertical slice

## Context

The `FAM1` encoder can represent one access unit, but the paired QUIC session
still only exposes one-shot control streams. Sending each media frame as a
control request would retain a 256-byte control limit and add needless stream
setup overhead.

## Decision

Expose a sender-owned unidirectional QUIC media stream on `PairingSession`.
Each serialized FAM1 frame is prefixed with a four-byte big-endian length and
must be non-empty and no larger than `MAX_MEDIA_FRAME_BYTES`. Send and receive
operations take explicit deadlines. The sender finishes the stream after the
last frame; the receiver treats an incomplete length or payload as a closed
connection rather than accepting partial media.

This layer does not parse codecs or FAM1 fields. The native adapter remains the
owner of media-frame validation, while platform code receives complete encoded
access units only after both bounds and stream framing have passed.

## Consequences

- FANP can carry multiple media frames without overloading control streams.
- Length validation prevents a peer from forcing an unbounded allocation.
- A future queue/flow-control adapter can pause producers without changing the
  wire framing or platform APIs.
