# ADR 0007: bounded media packet contract

- Status: Accepted
- Date: 2026-10-09
- Scope: M1 Native vertical slice

## Context

The discovery and FANP control layers now establish an authenticated session,
but platform renderers still need a common ownership and timing contract. If
each Android, desktop, and test sender invents its own packet type, queue
limits and timestamp semantics will drift before the first end-to-end media
session exists.

## Decision

Add `frameark-media` as a platform-independent Rust crate. It owns bounded
encoded `VideoFrame` and `AudioPacket` values, explicit track timestamps, and a
FIFO `MediaQueue` with packet-count and byte limits. A full queue returns an
explicit error; it never silently drops media. The protocol adapter decides
whether to pause, request a keyframe, or close the session.

The crate does not decode, render, capture, or select a codec. The current
packet enum carries the negotiated track and leaves codec ownership to the
session/offer layer. Platform bindings may borrow payload bytes but must not
mutate or retain them after the owning packet is released.

## Consequences

- Android, desktop, and reference senders share one bounded ownership model.
- Backpressure is observable and testable at the protocol boundary.
- Codec-specific behavior remains outside this contract and can evolve without
  coupling the core to MediaCodec, VideoToolbox, FFmpeg, or a UI toolkit.
- The next M1 increment can map FANP media stream records into these packets
  without changing platform APIs.
