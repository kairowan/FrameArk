# Dependency notice generation and review

## Coverage and evidence

The release workflow resolves both applications' `releaseRuntimeClasspath` via
Gradle's artifact API, recording coordinates and actual JAR hashes. It does not
scan the whole Gradle cache or treat a POM-only list as the resolved runtime.
The private JSON reports contain local file paths and are **not published**.
Module/file/project dependency shapes outside the reviewed policy fail closed.

`scripts/third_party_notices.py` checks Cargo metadata against every external
package in `Cargo.lock`. For each crate it verifies the original cached `.crate`
archive against the lockfile checksum and the resolved manifest against that
archive. Legal files, including nested NOTICE/LICENSE/AUTHORS files, are read
without extracting archive paths. It is deliberately a complete lockfile
superset, not a claim that all dependencies are linked into the Android binary.

The `jni-sys-macros` 0.4.1 and `yasna` 0.5.2 archives omit license bodies. Their
MIT supplements are pinned to the exact `.cargo_vcs_info.json` upstream commit.
`r-efi` supplies the full MIT option and copyright in `AUTHORS`; it is retained,
not mistaken for a crate with no license. The selected permissive options and
conjunctions are explicit in [policy.json](third-party/policy.json); arbitrary
GPL-only/new expressions are not silently admitted.

Kotlin 2.2.20 JVM/common stdlib includes notices for GWT, Guava, ThreeTenBP and
Boost, not just the top-level Apache license. Its pinned upstream license README
and compiler-wide NOTICE are retained for context; this does not mean the compiler
is bundled. Annotations 13.0 preserves all four distinct source-header copyrights,
including Sascha Weinreuter. Both exact runtime JAR hashes are checked. Supplements
have provenance and SHA-256 values; a version bump requires renewed review.

Rust's shipped `COPYRIGHT-library.html` contains the standard-library and bundled
dependency terms. The original HTML and Apache/MIT texts are retained; a plain-text
view is included in the combined notice. NDK `NOTICE` and `NOTICE.toolchain` are
included as a conservative runtime/compiler-runtime superset. Supply the **same
NDK root used to build the JNI library**, not an arbitrary installed NDK.

## Developer commands

From the repository root, after building the normal ARM64 Rust JNI bridge:

```sh
rustup component add rust-docs
./gradlew --no-daemon writeReleaseRuntimeInventories
python scripts/third_party_notices.py --output build/third-party-notices/assets --ndk-root "$ANDROID_NDK_LATEST_HOME"
./gradlew --no-daemon assembleRelease -PframearkVersionName=0.0.0-ci -PframearkVersionCode=2
python scripts/android_release_smoke.py --sdk-tools "$ANDROID_HOME/build-tools/35.0.0"
```

Windows developers use `gradlew.bat`, an explicit local NDK/build-tools path, and
quote the entire `-PframearkVersionName=0.0.0-ci` argument in PowerShell. For a
new generation, choose a fresh output directory and pass its path with
`-PframearkNoticesDir=build/<fresh-name>/assets`. The generator refuses to overwrite
an existing output directory; it does not recursively delete user data.

Gradle embeds the three generated files under `assets/` in both APKs. They are
available by treating the APK as a ZIP; this increment does not add an in-app
license viewer. A local development build may compile without generated notices,
but it is **not a distributable build** and the release verifier rejects it.
For debug preview delivery, generate/embed the same notices before packaging.

The signing preparation extracts the verified notice set from the actual APK,
requires identical sets in Sender and Receiver, and binds all three hashes in
`android-inputs.json` schema 2. Signing rechecks APKs and sidecars before secrets,
then publishes the same sidecars and includes their hashes in `android-signing.json`
schema 2. Old schema 1 artifacts must be rebuilt, not upgraded by editing JSON.
No already-published production schema-1 release existed at this transition.

## Failure and regression coverage

Generation or signing fails for missing lockfile packages, unreviewed runtime
versions/graphs/licenses, changed crate/JAR/supplement hashes, missing runtime
notices, incomplete license bodies, stale indexes, different APK notice sets,
modified readable/original texts, duplicate archive members, traversal, links and
size-limit violations. Archives are read in memory within explicit bounds and
never extracted into the filesystem. Public indexes omit all local cache paths.

ZIP members have sorted paths, fixed timestamps and fixed permissions. Identical
dependency, policy and toolchain inputs produce identical outputs; a different NDK
notice version is intentionally a different input. CI builds real APKs and signs
them with a disposable key; a real APK with a removed notice is also rejected.

Run `python -m unittest discover -s scripts -p 'test_*.py'` for the negative cases.
Do not suppress a missing notice or use a generic template in place of an upstream
copyright notice just to make CI pass. Review new dependencies, update pinned
evidence and tests, then use the normal focused-branch/PR process.

Source references: [Apache redistribution conditions](https://www.apache.org/licenses/LICENSE-2.0),
[Kotlin's versioned third-party inventory](https://github.com/JetBrains/kotlin/blob/693c44ee79f62895a9b92bdd60fdd7a9bc29a975/license/README.md).
Legal, patent, trademark and platform-distribution review remain separate gates.
