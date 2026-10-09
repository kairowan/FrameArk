# ADR 0008: bounded FANP media-frame wire format

- Status: Accepted
- Date: 2026-10-09
- Scope: M1 Native vertical slice

## Context

`frameark-media` now defines ownership, timestamp, payload and backpressure
contracts, but a sender and receiver still need a deterministic representation
for one encoded access unit. Control messages must not be overloaded with
unbounded media data, and a decoder must reject trailing or oversized bytes
before they reach a platform codec.

## Decision

`frameark-native::media_wire` defines a versioned `FAM1` envelope for exactly
one video or audio access unit. It carries the track identifier, sequence,
presentation timestamp, optional video decode timestamp, time base, audio
duration and bounded payload. All integers are big-endian. The decoder requires
an exact frame boundary, validates the reserved flags, bounds the track and
payload before allocation, and delegates final construction to
`frameark-media`.

Codec selection remains in the negotiated `SessionOffer`; this envelope does
not duplicate codec identifiers or silently transcode data. It is a wire
building block, not a claim that any platform decoder or hardware path is
already implemented.

## Consequences

- FANP can carry one encoded access unit without coupling protocol code to
  Android, desktop, FFmpeg or a rendering API.
- Parser fuzzing can target one small bounded frame at a time.
- A future stream multiplexer can add framing and flow control around this
  envelope without changing timestamp semantics.
