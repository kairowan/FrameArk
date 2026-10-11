# ADR 0059: Android FANP mDNS publication

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver discovery runtime

## Decision

When the Rust-owned FANP listener starts, `frameark-ffi` starts a bounded
`frameark-discovery` publisher on a separate worker. It advertises the
`_frameark._udp.local.` service with the temporary port, `proto`, `ver`, `tls`,
and media capability labels. The pairing code, certificate, device identity,
and peer addresses are never placed in TXT metadata.

Publication is best effort: if Android's mDNS backend cannot initialize or
register, the secured FANP endpoint remains available for an explicit IP/host
flow. On stop, the registration is withdrawn and the discovery daemon is
shut down before the listener worker is joined.

## Verification

Rust workspace tests continue to cover bounded discovery field/TXT validation
and the FFI listener start/stop path. CI Android builds exercise the
`frameark-discovery` dependency; physical multicast delivery and Android
network permission behavior remain device-matrix work.
