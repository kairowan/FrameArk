# ADR 0118: Enforce stopped-only trust mutation in Rust

Status: Accepted

## Decision

The JNI listener takes a startup snapshot of the trusted-key registry. All
public trust add/refresh, raw-key enrollment and revoke entry points therefore
use one Rust mutation gate under the receiver lock. Running or stopping returns
`-5` before the registry is touched. Kotlin's Boolean result remains false for
all rejection codes; the settings-controller preflight is not the authority.

Shutdown marks the lifecycle stopping before moving the listener out of the
state lock. It joins outside that lock so media callbacks can finish, then
clears residual media/config/proof state and publishes stopped. During this
interval running queries stay true; restart, concurrent stop, mode changes,
identity replacement and snapshot import remain unavailable. Duplicate stop
after completion is still successful. No lock is held across listener join.

## Evidence and limits

A regression first reproduced successful revoke during an active lifecycle.
It now rejects without changing snapshot bytes. A real-listener shutdown test
covers the network-only gate, join interval, concurrent stop, restart, snapshot
and identity replacement, final cleanup and successful stopped revocation.
Kotlin tests verify all three mutation wrappers report native busy as failure.

This preserves the existing stopped-only policy; it does not add live revoke,
disconnect an already-admitted peer on a failed mutation, enroll peers from
unverified names, or change the persisted `FTR1` format. Applications must wait
for stop completion, then revoke and persist through the existing controller.
Rollback should rebuild paired APK/JNI and retain the stopped-only restriction.
