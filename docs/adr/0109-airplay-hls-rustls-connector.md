# ADR 0109: Explicit-root Rustls connector for AirPlay HLS

- Status: Accepted
- Date: 2026-10-12
- Milestone: M4 RAOP/AirPlay media

## Decision

`frameark-airplay` exposes an explicit-root Rustls connector for HLS playlist
and segment fetches. The caller supplies bounded DNS results, trust anchors,
network-class policy, and timeout. Convenience fetchers reject plain HTTP,
preserve the existing bounded response parser, and never follow redirects.
The connector verifies the HLS authority as TLS SNI and returns a `Read + Write`
stream only after the handshake completes.

## Security consequences

The connector does not load operating-system roots, perform implicit DNS, pass
credentials, or bypass certificate verification. Private/loopback addresses
require an explicit `AllowAny` policy; public callers should use the narrowest
appropriate policy and root store. Pairing, FairPlay, media decryption,
decoding, and Apple interoperability remain outside this protocol layer.

## Evidence

Local Rustls fixture tests cover successful playlist and segment fetches, plain
HTTP rejection before resolution, wrong-root failure, and SNI mismatch failure.
The repository CI matrix runs the complete AirPlay test suite and policy,
security, and platform checks.
