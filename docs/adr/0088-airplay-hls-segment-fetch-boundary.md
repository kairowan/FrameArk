# ADR 0088: AirPlay HLS segment fetch boundary

- Status: Accepted
- Scope: M4 AirPlay URL media

## Context

The AirPlay module previously parsed HLS playlists through a caller-owned
connector but left segment retrieval entirely unspecified. Implementations
that filled that gap ad hoc could accidentally follow redirects, bypass DNS
policy, or retain an unbounded segment body.

## Decision

Add `fetch_hls_segment_with_connector` as a bounded one-request boundary. It
resolves a validated relative segment against the playlist path, accepts
explicit HTTP(S) absolute segment URLs, sends one `GET`, rejects non-2xx
responses (including redirects), and caps the encoded response at 2 MiB plus
bounded headers. DNS, network class, TLS, authentication, cancellation,
connection reuse, cache eviction, and decoding remain caller-owned.

## Consequences

- A daemon can implement a safe segment fetch/cache loop without duplicating
  URL and response parsing rules.
- The protocol crate still does not claim FairPlay/decryption or decoder
  support, and it never silently follows redirects.
- Production URL playback remains conditional until a vetted connector,
  decoder, and named Apple-device regression matrix are recorded.
