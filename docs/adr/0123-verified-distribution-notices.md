# ADR 0123: verified third-party notices in Android distributions

- Status: Accepted for release engineering; not legal/patent clearance
- Scope: M0 distribution evidence; no protocol, FFI or runtime-media change

## Decision

A project license plus a Rust-only SBOM is insufficient evidence for the APK
distribution: JVM runtime attribution, nested upstream notices and bundled
toolchain runtime terms were absent. Use a bounded, offline generator fed by
locked Cargo archives, actual Gradle runtime resolution and the build toolchain's
notice files. Include every locked Rust dependency as a conservative superset.
Do not infer exact binary linkage from metadata or hide build/target-only entries.

Verify crate archive hashes against Cargo.lock, exact runtime JAR hashes against
reviewed version policy, and supplemental license text hashes/upstream revisions.
Preserve nested legal files. Missing-license supplements use fixed upstream source
revisions, never mutable main URLs or invented copyright templates. Record explicit
license choices for existing permissive OR branches and all required AND terms.
Unknown expressions/components require review; the check is not legal advice.

Both APKs embed a readable notice, machine-readable index and original-file ZIP.
Preparation/signing verify coverage, current checkout hashes, byte integrity,
archive bounds and equality across APKs. The same sidecars are extracted from the
verified APK and published with it. Release input/receipt schemas move to 2 and
bind notice hashes; schema 1 is rejected. No production schema-1 release existed.

The unsigned build does all resolution/generation. Signing does no dependency
download, archive extraction or Gradle execution and still has no insecure fallback.
Generation refuses an existing output path; any validation failure occurs before
creating output. Filesystem failures cannot yield an accepted partial distribution.
No production key, tag or release is created by this change.

## Verification and limits

Initial local generation caught the incorrect assumption that normal Cargo cache
trees contain vendor-specific `.cargo-checksum.json`; checksum verification now
uses the original `.crate` archives. Real current inputs produced 123 components
and 255 original legal files. Unit fixtures cover determinism, missing coverage,
tampering, stale state, archive traversal/duplicates/links/bounds and missing runtime
notices. Actual ARM64 dual-APK build/sign smoke verifies embedded/sidecar identity
and rejects a real APK with a required legal asset removed.

The PR whitespace gate caught two trailing-whitespace lines in Kotlin's upstream
license README. Normalize those lines without changing attribution or terms,
update the stored-file hash and retain the original upstream LF-normalized hash
for provenance. Verify both the staged diff and the complete PR diff, regenerate
the notices and rebuild both APKs after this policy change.

A follow-up verifier review reproduced eight policy-coverage omissions: an index,
readable text and ZIP could be rewritten consistently after dropping/changing a
required Maven/Rust supplement or dropping a runtime notice. The signer now checks
exact Maven/runtime file inventories, mandatory Rust supplements, reviewed hashes
and provenance, component ownership and Maven license metadata independently of
the supplied index. Synthetic regressions reproduce the original failures. This
does not replace trust in the reviewed no-secret build or authenticate arbitrary
crate/toolchain content without that build's evidence.

No in-app license viewer, exact linker map, expanded Android SPDX SBOM or legal
clearance is claimed. System codecs/DRM and trademark/patent gates remain separate.
M0–M5 maturity does not advance merely because packaging checks pass. Rollback is
a reviewed PR; do not restore releases that silently omit third-party notices.

See [operations](../DEPENDENCY_NOTICES.md) and [root notice policy](../../THIRD_PARTY_NOTICES.md).
