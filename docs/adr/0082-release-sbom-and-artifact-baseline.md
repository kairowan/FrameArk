# ADR-0082: Publish a deterministic SBOM and verified release artifacts

## Status

Accepted — M0 release-governance increment.

## Context

The release workflow already validated tags, source archives, Rust tests, and
Android packaging, but it published only a source zip and checksum file. A
reviewer could not inspect the dependency inventory or download the release APK
from the generated release, and the checksum did not cover every artifact.

## Decision

`scripts/generate_sbom.py` creates an SPDX 2.3 document from locked Cargo
metadata with deterministic package IDs, dependency relationships, license
fields, and a `SOURCE_DATE_EPOCH`-controlled creation time. The release job
generates that SBOM, copies the verified Android release APK, and hashes every
file in `dist/` (including the SBOM, source archive, and APK) before publishing
all of them to GitHub Releases.

The SBOM describes the Rust workspace and its resolved dependencies; it does
not claim to replace legal review of Android SDK, optional codec, or platform
components. Release signing and provenance attestation remain separate future
work and no unsigned artifact is called Stable.

## Verification and limits

The generator is dependency-free Python and is exercised against locked Cargo
metadata in repository checks. Release still requires the existing Rust,
Android, native-package, repository, and tag-ancestry gates. No release is
published from an unreviewed commit or a non-semantic tag.
