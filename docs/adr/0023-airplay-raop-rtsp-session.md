# ADR 0023: Bounded RAOP RTSP session negotiation

- Status: Accepted
- Date: 2026-10-10
- Scope: M4 AirPlay/RAOP audio foundation

## Decision

Add a dependency-free RAOP session state machine to `frameark-airplay`. It
parses bounded ANNOUNCE SDP for a single RTP payload and PCM/ALAC/AAC format,
validates a UDP `RTP/AVP/UDP;unicast;mode=record` SETUP, and enforces
OPTIONS → ANNOUNCE → SETUP → RECORD → FLUSH/GET_PARAMETER → TEARDOWN ordering.
The caller supplies the UDP server port and owns all sockets, packet timing,
and decoder resources.

Responses preserve CSeq and expose a deterministic local session identifier;
unsupported transitions, codecs, payload mismatches, TCP interleaving, and
invalid transports fail closed before state mutation.

## Evidence and limits

Tests cover ALAC SDP negotiation, response headers, the complete bounded
session lifecycle, unsupported codecs, invalid transport, and out-of-order
requests. Pairing, plist/FairPlay, encryption, RTP retransmission/timing,
audio decoding, and Apple-device compatibility remain separate work and are
not inferred from this state machine.
