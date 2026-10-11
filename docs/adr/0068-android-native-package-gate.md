# ADR 0068: Android JNI package verification gate

- Status: Accepted
- Date: 2026-10-11
- Scope: M0/M1 Android release packaging

## Decision

After the Android CI and release workflows build the Rust JNI bridge and APK,
run `scripts/verify_android_native_package.py` against the produced artifact.
The gate requires a valid ZIP/APK containing a non-empty
`lib/arm64-v8a/libframeark_ffi.so`. It runs for both debug CI artifacts and
release artifacts, matching the only ABI currently declared by the receiver.

This catches the failure mode where Rust compilation succeeds but Gradle
packages an APK that cannot load `frameark_ffi`. It does not claim that the
native library has been loaded on physical hardware; device install/load and
hardware decoder evidence remain separate requirements.

## Verification

The verifier is deterministic and uses only Python's standard-library ZIP
reader. CI and release invoke it immediately after `assembleDebug` or
`assembleRelease`; malformed, missing, or zero-byte entries fail the job.
