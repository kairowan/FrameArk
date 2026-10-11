# ADR 0092: DLNA bounded remote media range fetch

- Status: Accepted
- Scope: M3 DLNA media push

## Context

The MediaRenderer can validate a remote HTTP(S) URI and probe its format, but
playback adapters still need a safe way to obtain media bytes. A generic URL
fetcher would make the protocol crate responsible for DNS, TLS, redirects,
authentication, cancellation, buffering, and potentially unbounded downloads.

## Decision

Add `fetch_media_range_with_connector` as a narrow caller-owned boundary. It
applies the existing DNS/network-class policy, emits one `GET` with an
optional inclusive `Range`, rejects redirects and non-2xx responses, requires
`206` plus an exact matching `Content-Range` when a range was requested, and
caps one response body at `MAX_MEDIA_FETCH_BYTES`. The connector remains the
owner of DNS resolution, TLS verification, authentication, cancellation and
connection reuse.

## Consequences

- Platform and daemon adapters can schedule bounded range reads for playback
  and Seek without copying a complete remote object into protocol state.
- A range-ignoring or mismatching origin fails closed instead of silently
  returning an unexpected full response.
- Full URL streaming, redirect policy, decoder inspection, cache eviction and
  named-client interoperability remain outside this increment.
