# ADR-0084: Attest reviewed release artifacts with GitHub OIDC provenance

## Status

Accepted — M0 release-governance increment.

## Context

The release workflow already validated tag ancestry, tested Rust and Android
outputs, generated a deterministic SBOM, and published checksums. A checksum
proves integrity only after a trusted download; it does not describe which
reviewed workflow produced the artifact. The repository also cannot safely
embed a maintainer's signing key in CI.

## Decision

The release job grants only the release workflow `id-token: write` and
`attestations: write` in addition to its existing `contents: write` permission.
After source, SBOM, APK, and `SHA256SUMS` are created, the official
`actions/attest-build-provenance@v2` action emits GitHub OIDC-backed build
provenance for every file under `dist/`. The same files are then published to
the GitHub Release.

This attestation identifies the workflow/build provenance; it is not an APK
signing key, Apple notarization, or a claim that optional platform codecs are
covered by the Rust SBOM. Platform signing and independent release review stay
explicit follow-up gates.

## Verification and limits

Repository policy validates the least-privilege workflow and the release job
continues to require a semantic tag reachable from `main`, full Rust checks,
Android packaging verification, SBOM generation, and checksums. A real release
tag is required to exercise GitHub's OIDC service; local tests validate the
workflow and scripts but cannot mint an attestation.
