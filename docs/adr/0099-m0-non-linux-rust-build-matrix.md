# ADR 0099: add Windows and macOS Rust CI evidence

- Status: Accepted
- Scope: M0 cross-platform engineering baseline

## Context

M0 had Linux CI and Android package verification, but no repeatable evidence
that the Rust workspace and reference receiver build on non-Linux hosts. A
single Linux result could hide platform-specific compiler, socket, or TLS
assumptions.

## Decision

Add a fail-fast-disabled GitHub Actions matrix for `windows-latest` and
`macos-latest`. Each leg runs Rust formatting, the complete workspace test
suite, and a `framearkd` build. The aggregate `ci-gate` requires the matrix
job, while branch protection continues to require only the stable aggregate
check rather than individual matrix names.

## Consequences

- M0 gains repeatable non-Linux core build/test evidence on every pull request.
- The matrix does not claim a running receiver, platform signing, hardware
  decoding, or a named device compatibility result; those remain open gates.
- A platform-specific failure blocks the aggregate gate and must be fixed or
  explicitly documented before merge.
