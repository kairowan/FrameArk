# ADR 0053: Reference receiver diagnostics export

- Status: Accepted
- Date: 2026-10-11
- Scope: M1 Native reference receiver diagnostics

## Decision

Integrate `frameark-diagnostics` into `framearkd` and add the explicit
`--diagnostics-out` option. The daemon records bounded startup, pairing, and
session outcome events and writes a deterministic support summary when the
process exits. The option is opt-in; the daemon does not upload telemetry or
persist diagnostics by default.

The reference daemon still uses counting renderers and a temporary pairing
session. Diagnostics improve stage visibility but do not turn the fixture into
a hardware decoder, production authorization service, or device-compatibility
claim.

## Verification

The daemon option parser and workspace checks cover the new control. The
diagnostics crate tests cover bounds, sanitization, deterministic output, and
privacy omissions.
