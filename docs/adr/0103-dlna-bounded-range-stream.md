# ADR 0103: stream DLNA URLs as bounded exact ranges

- Status: Accepted
- Scope: M3 DLNA media push

## Context

The single-range connector protected the renderer from unbounded responses, but
callers still had to duplicate range scheduling for a complete known-length
resource. Duplicated schedulers could skip bytes, overlap ranges, or accept an
origin that ignored `Range`.

## Decision

Add `stream_media_ranges_with_connector`. It validates the URI/network policy
once, requires a caller-supplied resource length, schedules contiguous
inclusive ranges up to a 512 MiB and 4096-request budget, and delegates each
response to a caller sink immediately. Every response must be `206` with an
exact `Content-Range`; redirects and range-ignoring origins fail closed.

The connector still owns DNS, TLS/certificate validation, authentication,
timeouts, cancellation, and connection reuse. The protocol crate buffers only
the current bounded response chunk and never follows a redirect.

## Consequences

- DLNA callers can implement full known-length URL playback and Seek without
  loading the complete object into protocol state.
- Unknown-length resources, oversized resources, and malformed/mismatched
  ranges remain explicit errors.
- Production TLS, decoder-backed format inspection, and named controller
  interoperability remain separate exit gates.
