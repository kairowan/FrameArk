# ADR 0051: AirPlay mirror bounded RTSP server

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen mirroring control transport

## Decision

Add `MirrorRtspTcpServer` as a small, caller-owned TCP adapter around the
bounded `MirrorSession` state machine. It accepts one RTSP request per
connection, applies a non-zero read deadline, limits request bytes and request
batch size, dispatches the parsed request to the session, writes the encoded
response, and closes the connection. The adapter exposes the resulting session
state for a higher-level media coordinator.

The server does not implement Apple pairing, FairPlay/AES-CTR encryption,
authorization, TLS, RTP sockets, or audio/video decoding. Callers must put it
behind the authenticated AirPlay boundary, derive the peer/SSRC constraints
from that boundary, and own lifecycle, network admission, and pipeline reset.
The one-request connection model is deliberate until a production connection
scheduler and authenticated persistent-session policy are specified.

## Verification

The AirPlay suite verifies loopback OPTIONS delivery, response encoding,
session-state observation, read-timeout configuration, and rejection of zero or
oversized request budgets. Existing RTSP parser/session, mirror coordinator,
and RTP pipeline fixtures remain required.
