# ADR 0019: RAOP RTSP and RTP audio foundation

- Status: Accepted
- Date: 2026-10-10
- Scope: M4 AirPlay/RAOP media compatibility

## Decision

Create `frameark-airplay` with bounded RTSP/1.0 message contracts and a
strict RTP version-2 audio packet representation. RTSP headers are normalized,
CSeq is parsed explicitly, content lengths are checked before body exposure,
and duplicate/oversized fields are rejected. RTP accepts only the initial
header form without padding or extensions until negotiated RAOP header
extensions have their own fixtures and limits.

The crate also names the experimental PCM, ALAC, and AAC codec choices without
implementing decoders. Pairing, FairPlay, AES-CTR audio decryption, plist
semantics, timing/feedback, and actual Apple-device interoperability remain
separate work.

## Verification

Unit tests cover RTSP request/body round trips, duplicate and content-length
rejection, and RTP round trips with extension/size rejection.
