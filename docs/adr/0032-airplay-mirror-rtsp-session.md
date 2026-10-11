# ADR 0032: Bounded AirPlay mirror RTSP session

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring control boundary

## Decision

Add `MirrorSession` to `frameark-airplay` as a Rust-owned, socket-free RTSP
state machine. It accepts OPTIONS, validates SETUP's UDP record transport and
XML/binary plist mirror configuration, advances through RECORD, echoes bounded
GET_PARAMETER bodies, accepts FLUSH while streaming, and closes on TEARDOWN.
The setup configuration requires bounded width, height, orientation, and audio
sample rate fields and creates the shared 90 kHz `MirrorClock` policy.

The session returns deterministic RTSP responses with CSeq, Session, Public,
and Transport headers. It does not open sockets, authenticate Apple senders,
decrypt FairPlay/AES payloads, decode H.264/audio, or render to Android/Desktop
surfaces. Those responsibilities remain in transport, trust, and platform
layers.

## Evidence and limits

Tests cover a complete OPTIONS → SETUP → RECORD → GET_PARAMETER → FLUSH →
TEARDOWN sequence, binary/XML configuration mapping, UDP port validation,
invalid dimensions, and transition/order errors. No Apple-device capture,
pairing transcript, decoder output, or loss/recovery integration is claimed.
