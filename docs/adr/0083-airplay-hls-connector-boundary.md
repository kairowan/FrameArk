# ADR-0083: Caller-owned AirPlay HLS playlist connector boundary

## Status

Accepted — M4 AirPlay URL/HLS interoperability increment.

## Context

`frameark-airplay` already parsed bounded HLS media playlists, but a daemon
could not reuse a protocol-owned response boundary without accidentally
coupling DNS, TLS, redirect, or authentication policy to the parser. A direct
URL fetcher in the protocol crate would also create an SSRF and credential
handling risk.

## Decision

Add `fetch_hls_playlist_with_connector`. It validates an absolute HTTP(S) URL,
rejects credentials and fragments, creates one origin-form GET request, and
parses one bounded response. The connector owns DNS, network-class policy,
certificate validation, authentication, redirect decisions, cancellation, and
socket timeouts. The crate rejects non-2xx responses, duplicate headers,
chunked transfer, oversized headers/bodies, and incompatible HLS MIME types,
then hands the bounded body to `HlsMediaPlaylist::parse`.

No segment is fetched, cached, decrypted, or decoded by this API. Redirects
must be handled by a higher layer that revalidates every destination. HTTPS is
only meaningful when the connector returns a stream with verified TLS; a plain
stream must not be used as an HTTPS implementation.

## Verification and limits

Tests cover HTTPS connector arguments, content-length and playlist parsing,
redirect rejection, credential-bearing authorities, incompatible MIME types,
and oversized response bodies. Apple pairing, FairPlay, segment fetch/cache,
decoder output, and device compatibility remain open M4 work.
