# ADR-0076: Add bounded H.265 mirror RTP assembly

## Status

Accepted — experimental M5 protocol increment. Supersedes the H.265 rejection
part of ADR-0072.

## Decision

Keep H.264 as the default when a mirror SETUP omits `codec`, and accept explicit
`h265`, `h.265`, or `hevc` as a conditional `MirrorVideoCodec::H265`. The
bounded `MirrorVideoPipeline` implements the RFC 7798 packet shapes needed by
the protocol boundary: single NAL units (types 0–47), aggregation packets
(type 48), and fragmentation units (type 49). FU reconstruction preserves the
forbidden/layer/temporal bits and replaces only the NAL type from the FU header.
IDR_W_RADL, IDR_N_LP, and CRA types (19–21) are exposed as keyframes.

The access-unit contract records the codec and remains caller-owned; no decoder,
surface, UDP socket, Apple pairing, FairPlay/AES-CTR decryption, or loss
concealment is added. H.265 setup and RTP data must remain behind an
authenticated RTSP/session boundary.

## Bounds and failure policy

The existing jitter capacity, NAL count, access-unit bytes, AP length, FU
length, timestamp, marker, and explicit sequence-gap recovery bounds apply to
H.265. Malformed two-byte headers, forbidden bits, invalid types, zero/short
AP entries, FU overlap, timestamp changes, and end-without-start are rejected
without emitting partial media.

## Verification and limits

Tests cover H.265 SETUP acceptance, Annex-B parsing, AP aggregation, reordered
FU assembly, keyframe detection, malformed AP input, and the existing H.264
regression suite. This does not provide Apple-device interoperability,
encryption, hardware decode, or a Beta compatibility claim.
