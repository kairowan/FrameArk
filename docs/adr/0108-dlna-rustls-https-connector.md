# ADR 0108: Explicit-root Rustls connector for DLNA media HTTPS

- Status: Accepted
- Date: 2026-10-12
- Milestone: M3 DLNA/media push

## Decision

`frameark-dlna` provides `connect_https_media_uri_with_roots` and the
lower-level `connect_https_with_roots` as a bounded Rustls transport seam. The
caller owns DNS resolution, the trust-anchor set, network-class policy,
cancellation, and redirect policy. The connector rejects non-HTTPS media URIs,
bounds roots and resolved addresses, connects only to the caller-provided
address set, verifies the URI host as TLS SNI, installs read/write timeouts,
and completes the handshake before returning a `Read + Write` stream.

## Security consequences

No operating-system trust store, implicit resolver, redirect, credential
forwarding, or certificate-verification bypass is used. A caller that accepts
private or loopback addresses must opt into `AllowAny`; production callers
should use the appropriate local/public policy and a narrowly scoped root
store. The connector does not claim DLNA controller interoperability or
decoder support.

## Evidence

The crate tests run a local Rustls server fixture and cover successful
explicit-root HTTPS, plain-HTTP rejection before resolution, empty-root
rejection, wrong-root failure, and SNI mismatch failure. CI runs the same
focused test suite with the repository Rust format, lint, security, and policy
gates.
