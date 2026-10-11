# ADR 0073: RAOP PCM16 platform boundary

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 RAOP audio media boundary

## Decision

Add a bounded `RaopPcm16AccessUnit` conversion after the ordered RTP
coordinator. RAOP `L16`/PCM payloads are network-byte-order 16-bit samples;
the conversion swaps each sample to little-endian bytes suitable for a
platform audio sink. It is available through `RaopAudioCoordinator::pop_pcm16`
and requires the negotiated PCM codec, 16-bit depth, payload type, and complete
interleaved channel frames.

The conversion does not decrypt, decode, conceal loss, resample, or own an
audio device. ALAC, AAC, FairPlay, and encrypted RAOP remain explicit follow-up
or unsupported boundaries.

## Verification

- Unit tests verify timestamp/channel preservation and two-channel endian
  conversion in sequence order.
- Unit tests reject non-PCM formats, wrong payload alignment, and malformed
  platform-boundary inputs.
- Compatibility and README text retain the Experimental label and no Apple
  device or long-run claim.
