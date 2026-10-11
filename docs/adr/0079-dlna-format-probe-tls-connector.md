# ADR-0079: Keep DLNA response probing and TLS callback transport caller-owned

## Status

Accepted — experimental M3 interoperability increment.

## Context

The DLNA renderer already generated bounded GENA `NOTIFY` requests and
validated media URI authorities, but it intentionally did not implement TLS,
URL fetching, or decoder inspection. Implementing those concerns inside the
protocol crate would create a second network policy, hide certificate
validation choices, and make a format report look like a decoder guarantee.

## Decision

Add two narrow boundaries:

1. `send_gena_callback_with_connector` accepts a validated HTTP or HTTPS URL,
   a caller-selected timeout, and a caller-owned `Read + Write` stream. Rust
   keeps the request/response bounds while the connector owns DNS, socket
   setup, TLS certificate verification, retries, and peer authorization.
2. `MediaFormatProbe::from_response` extracts a bounded MIME type, optional
   `DLNA.ORG_PN` profile, and content length from an already-fetched HTTP
   response. `is_supported_by` compares only the MIME type with the declared
   `ConnectionManager` sink list.

The existing `GenaCallbackClient` remains a plain-HTTP convenience path and
continues to reject HTTPS. Neither new API follows redirects, fetches a media
URL, inspects a platform decoder, or authenticates a sender.

## Verification and limits

Unit tests cover HTTPS connector delegation, bounded response parsing, MIME and
profile extraction, malformed metadata, and capability matching. The M3 exit
gate remains open until a production TLS connector, URL fetch/redirect policy,
decoder-backed probing, named controller matrix, and long Seek run are
recorded.
