# ADR-0077: Add caller-injected DNS-aware DLNA media URI policy

## Status

Accepted — experimental M3 security increment.

## Context

The DLNA renderer validated URL syntax and authority shape but deliberately did
not fetch media URLs. Future daemons and platform adapters may add URL fetching;
accepting a hostname without checking its resolved addresses would allow a
malicious sender or DNS answer to steer a fetcher toward loopback, multicast,
or private services. DNS behavior and network permissions belong to the caller,
not the protocol parser.

## Decision

Expose `validate_media_uri_network` with a caller-supplied resolver closure and
three explicit policies: `AllowAny`, `LocalNetworkOnly`, and
`PublicNetworkOnly`. Literal IPv4/IPv6 authorities are checked directly;
hostnames are resolved exactly once by the caller. The result is bounded to
`MAX_MEDIA_RESOLVED_ADDRESSES`, must be non-empty, and every address must be
usable (not unspecified or multicast). Public/local policy requires all
addresses to fit the selected class, preventing mixed DNS answers from being
silently accepted.

The crate does not perform DNS, connect, follow redirects, cache answers, or
serve as an SSRF-complete fetcher. Fetchers must use bounded DNS and connection
timeouts, pin or revalidate the chosen address, and apply redirect policy at
each hop.

## Verification and limits

Tests cover public and private multi-address answers, mixed-answer rejection,
literal loopback/multicast/IPv6 handling, empty and oversized resolver output,
and the existing URI parser. Named DLNA controller compatibility, HTTPS/TLS,
format probing, and actual media fetching remain open M3 work.
