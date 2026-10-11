# ADR 0094: bounded AirPlay HLS segment cache

- Status: Accepted
- Scope: M4 AirPlay URL media foundation

## Context

The HLS connector now returns bounded encoded segments, but a playback adapter
needs a deterministic way to reuse a segment without retaining an unbounded
playlist or allowing a burst of fetched media to exhaust memory. Cache policy
must remain separate from DNS, TLS, redirects, authentication and decoding.

## Decision

Add an opt-in `HlsSegmentCache` with a caller-selected byte budget capped by a
project maximum and a fixed maximum entry count. URI keys are validated using
the same HLS traversal and credential policy as the playlist parser. Reusing a
URI replaces its old bytes; inserting a new segment evicts the oldest entries
until both budgets hold. Empty, oversized, unsafe, or non-2xx response entries
are rejected.

## Consequences

- HLS playback adapters have a bounded in-memory reuse primitive and explicit
  eviction behavior that can be tested without a network or decoder.
- The cache is FIFO rather than an adaptive CDN policy and is not persisted;
  callers still own cancellation, authentication, timing, and output.
- FairPlay/encrypted media, production URL policy, decoder integration and
  device compatibility remain outside this increment.
