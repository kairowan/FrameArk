# ADR 0060: Android multicast-lock lifecycle

- Status: Accepted
- Date: 2026-10-11
- Scope: M2 Android Receiver Wi-Fi discovery boundary

## Decision

The Android foreground service owns a `WifiManager.MulticastLock` coordinator
for the active receiver lifecycle. It attempts one non-reference-counted lock
when the service starts, releases it on explicit stop and destruction, and
never passes the Android object into Rust. The coordinator is idempotent and
reports acquisition failure without blocking FANP, because Ethernet and
platforms without a usable Wi-Fi manager remain valid transport paths.

## Verification

JVM tests cover single acquisition/release, repeated lifecycle calls, and
best-effort failure handling. The physical Wi-Fi power/multicast matrix is
still required before claiming reliable Android mDNS delivery.
