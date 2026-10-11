# ADR 0043: bounded AirPlay RTP socket adapter

- Status: Accepted
- Scope: M4/M5 AirPlay/RAOP media transport boundary
- Date: 2026-10-11

## Context

The AirPlay crate already validates RTSP/RTP envelopes and exposes sequence-aware
audio and mirror-video pipelines, but callers had to duplicate UDP datagram
allocation and parse handling. That made it easy for a daemon integration to
forget a packet-size bound or to accidentally treat a parser as an
authentication boundary.

## Decision

`frameark-airplay` now exposes `RtpReceiverSocket`. It binds a caller-selected
UDP address, applies one bounded receive buffer, parses exactly one RTP v2
datagram, and returns the source address with the validated packet. Read
timeouts and operating-system failures are returned as explicit errors.

The adapter does not join multicast groups, connect or filter a peer, validate
SSRC or payload type, decrypt protected media, or advance a jitter buffer. The
RTSP/session owner remains responsible for those negotiated policies and for
placing the socket behind authorization and lifecycle timeouts.

## Consequences

- RAOP and mirror integrations can reuse one bounded socket boundary and test
  it with loopback UDP packets.
- A malformed datagram is rejected without entering a media pipeline, and an
  oversized datagram cannot cause an unbounded allocation.
- The feature does not increase the compatibility claim: Apple pairing,
  FairPlay/AES-CTR, decoder integration, peer authorization, and device
  interoperability remain Experimental or unsupported.

