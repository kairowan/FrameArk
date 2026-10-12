# ADR 0107: Classify RAOP encryption before unencrypted media handling

- Status: Accepted
- Date: 2026-10-12
- Milestone: M4

## Context

The experimental RAOP audio pipeline handles only bounded, unencrypted RTP
contracts. RAOP ANNOUNCE SDP can advertise AES-CTR material (`rsaaeskey` and
`aesiv`) or other protection attributes. Without a classification boundary, a
caller could accidentally feed protected bytes into PCM conversion or report a
protected session as a generic parser failure.

## Decision

Add `classify_raop_encryption` and `RaopEncryptionStatus`. The classifier
validates only the existing SDP size/line bounds and records the presence of
known protection attribute names. It never copies or logs key, IV, or other
attribute values. The result provides stable redaction-safe reason codes and
messages for:

- experimental clear media;
- RAOP AES-CTR attributes, which are unsupported;
- other or unknown protected media, which is unsupported.

The caller must check this result before connecting the unencrypted RTP
coordinator. No decryption, pairing, or FairPlay implementation is implied.

## Consequences

Protected sessions now fail with a deterministic compatibility reason instead
of silently entering the clear decoder boundary. The crate remains
`Experimental`; Apple pairing, production cryptography, decoders, and device
interoperability remain open M4 work.

## Evidence

`raop_encryption_classifier_exposes_clear_and_protected_boundaries` covers
clear, AES-CTR, other-protection, and malformed SDP cases. Compatibility and
status documents record the same boundary.
