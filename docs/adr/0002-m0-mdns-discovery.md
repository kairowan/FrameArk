# ADR 0002: M0 mDNS discovery adapter

- Status: Accepted
- Date: 2026-10-09
- Decision owners: FrameArk maintainers

## Context

The first Native Protocol milestone needs a local-network discovery primitive
before pairing and QUIC can be implemented. Discovery must work across active
IPv4/IPv6 interfaces, expose bounded metadata, and remain independent of the
session state machine and platform rendering APIs.

## Decision

1. Reserve `_frameark._udp.local.` for FrameArk Native Protocol endpoints.
2. Use the `mdns-sd` adapter behind a `frameark-discovery` crate. The crate
   publishes with automatic address tracking and supports an optional allowlist
   of interface names.
3. Convert backend events into `DiscoveryEvent` and `DiscoveredService`
   values before they reach `frameark-core` or a platform binding.
4. Bound instance/host/TXT fields and reject empty ports or invalid TXT keys
   before touching the network.
5. Keep the network smoke test opt-in because multicast availability is a host
   and CI-network property; deterministic validation remains in the crate unit
   tests.

## Consequences

- Android can later map the daemon lifecycle to `MulticastLock` without owning
  DNS-SD parsing or service state.
- The same discovery contract can be used by desktop and `framearkd`.
- `_frameark._udp` remains a working protocol identifier, while the `.local.`
  suffix is added at the DNS-SD boundary.
- A future transport implementation must validate the discovered TXT metadata
  again before opening a session; discovery is not authorization.

## Security notes

Discovery data is untrusted input. The adapter limits field sizes, does not
interpret URLs or credentials, and does not grant trust or start a session.
Pairing and transport authentication remain separate M0/M1 work items.
