# ADR 0100: add a bounded non-Linux receiver startup smoke

- Status: Accepted
- Scope: M0 platform runtime/load evidence

## Context

The Windows/macOS matrix proved that the Rust workspace and `framearkd` build,
but a successful compile alone does not show that the reference receiver can
load its binary and bind a local QUIC listener on those hosts.

## Decision

After the matrix builds `framearkd`, CI invokes
`scripts/platform_runtime_smoke.py`. The runner starts the executable with a
loopback ephemeral UDP port and bounded pairing configuration, observes that it
remains alive for three seconds, then terminates it and cleans up. It accepts
the Windows `.exe` suffix automatically and reports early process exit output.

## Consequences

- M0 records repeatable non-Linux binary-load and listener-start evidence.
- The smoke does not pair a sender, decode media, prove signing, or validate a
  named device; those remain separate exit gates.
- Startup failures fail the aggregate `ci-gate` rather than being silently
  ignored.
