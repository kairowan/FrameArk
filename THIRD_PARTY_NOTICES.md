# Third-party notices

FrameArk's own code is `Apache-2.0 OR MIT`. Third-party components retain their
respective terms and copyright notices; the project license does not replace them.

Every distributable Android build must include these generated assets, also
published beside the two signed APKs:

- `THIRD_PARTY_NOTICES.txt`: readable combined attribution and license text.
- `third-party-index.json`: component identities, selected Rust license options,
  provenance and file hashes; no local cache paths.
- `third-party-licenses.zip`: original legal files and the same index. Rust crate
  files retain their archived bytes; reviewed supplemental texts normalize LF and
  final newline only, with hashes and pinned upstream sources recorded in policy.

The generator covers the **complete Rust lockfile superset**, the **resolved
release runtime of each Android app**, Rust standard-library notices and the
build NDK's runtime/toolchain notices. A listed dependency is not necessarily
linked into every APK: host, target-conditional and build-time dependencies may
appear in the conservative Rust/runtime superset.

See [generation and review operations](docs/DEPENDENCY_NOTICES.md) and the
[reviewed policy](docs/third-party/policy.json). The release gate rejects absent,
stale, incomplete or modified notice sets before loading signing secrets.

These checks provide distribution evidence, not a legal opinion, trademark
clearance, codec patent license or proof of every platform's redistribution rights.
The publication review in `PLAN.md` remains required, including for prereleases.
