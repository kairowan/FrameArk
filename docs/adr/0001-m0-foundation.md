# ADR 0001: M0 foundation identifiers and ownership

- Status: Accepted
- Date: 2026-10-09
- Scope: Rust workspace, Android Receiver, FANP discovery identity

## Decision

FrameArk freezes the following names for the first implementation milestone:

| Concern | Decision |
|---|---|
| Rust crate prefix | `frameark-` |
| Android application ID | `dev.frameark.receiver` |
| Android namespace | `dev.frameark.receiver` |
| Native discovery service | `_frameark._udp` |
| Native protocol working name | FrameArk Native Protocol (`FANP`) |
| Rust core license | `Apache-2.0 OR MIT` |

`frameark-core` owns device, capability, session, track, event, error, and
lifecycle contracts. `frameark-api` owns platform-facing media, clock, and
renderer interfaces. Protocol adapters and platform applications depend on
these contracts; the core does not depend on Android APIs, a codec library, or
an asynchronous runtime.

## Rationale

The package and namespace are independent of a personal account or a platform
vendor. `dev.frameark.receiver` is an implementation identifier and does not
make a domain-ownership claim. FANP remains a project-controlled protocol name
until its wire specification is published.

## Consequences

- Future Android modules must use the frozen namespace and application ID unless
  a migration document and release note describe a deliberate package move.
- The discovery and protocol names are still pre-release identifiers; changing
  them before the first public client is cheaper than maintaining a compatibility
  alias after release.
- A future FFI crate may depend on `frameark-api`, but Android code must not
  reproduce the Rust session state machine.
- `frameark-ffi` is the only crate allowed to use the unsafe attribute required
  for a JNI export. Its entry points remain small, versioned, and free of raw
  pointer dereferencing until the media bridge is designed.
