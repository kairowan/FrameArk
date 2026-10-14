# ADR 0126: Retain authenticated receiver sessions on route notifications

- Status: Accepted, Experimental
- Date: 2026-10-14
- Supersedes: automatic restart behavior in ADRs [0070](0070-android-session-recovery-seam.md)
  and [0075](0075-android-network-observer-recovery.md)

## Problem

The default-network observer called `ReceiverSessionCoordinator.recover()` on
loss/availability. That stopped playback and Rust, closed every QUIC stream,
rotated the ephemeral certificate/PIN/port, and started a new listener. It did
not resume the sender. A still-reachable LAN session could be terminated just
because the default route changed; even a recoverable short loss was made
terminal by the platform callback.

## Decision

Replace that pre-alpha Kotlin seam with `onNetworkChanged()`: while the service
lifecycle is Running, refresh only the existing JNI endpoint view. No native
start/stop, playback reset, new authentication, deadline extension or success
claim occurs. Missing endpoint information returns false without silently
creating a replacement session. Stopped/failed states ignore notifications.
Remove the misleading platform `Recovering` state. Rust's shared core state
model and all existing wire/FFI contracts remain unchanged.

The coalescer uses `markChangeHandled()` (in a service-handler `finally` block).
It still unregisters on stop; late notifications cannot start anything. Logs
contain only endpoint-view availability, never network names, addresses, PINs,
certificates or media. Registration failure leaves native transport active.

Rust/Quinn retains responsibility for authenticated connection continuity and
bounded I/O failure. Existing application deadlines and queues are unchanged:
Android receive operations use 30 seconds, sender media writes 5 seconds,
producer admission 1 second, sender queue 128 frames / 8 MiB. These limits are
not a promised outage tolerance. An explicit user stop still closes/joins the
native listener, clears the endpoint and resets outputs. A subsequent explicit
start creates fresh pairing material.

## Security and compatibility

Network loss no longer implicitly rotates temporary pairing material; its
lifetime is the explicitly running receiver lifecycle. Stop the receiver to
end that authorization boundary. This change never makes a new peer trusted,
skips PIN/proof, resumes a closed stream or changes persistent trust. Do not
reuse bytes after a failed partial media write: callers still abort on failure.

No new dependency, permissions, cryptography, wire message or storage format.
Internal Kotlin callers migrate from `recover`/`markRecoveryComplete` to
`onNetworkChanged`/`markChangeHandled`; no native ABI change.

## Evidence and remaining work

The old restart failed the lifecycle preservation regression before the fix.
JVM tests cover callback storms, endpoint snapshot failure, late callbacks,
normal stop and playback failure. An isolated Android test exercises the real
JNI listener with injected notifications, stable certificate/PIN/port and
fresh certificate after explicit restart.

Rust's real localhost UDP relay drops all traffic during two one-second
blackouts on one pinned QUIC control/media connection. Tests verify exact
payloads/timestamps, one Offer/Start/Stop and one output cleanup, plus deadline
failure and cancellation cleanup. Opaque H.264/AAC-labelled payloads reach
renderer test doubles: **not** codec output, full-rate Android capture, latency,
Wi-Fi address migration, or physical M2 acceptance.

Full resume grants, rediscovery/address changes, OS network binding, codec
keyframe/resync, one-hour soak and reference-device weak-network evidence remain
open. See [operator test](../testing/ANDROID_NETWORK_CONTINUITY.md).

## Primary references

- [Android NetworkCallback](https://developer.android.com/reference/android/net/ConnectivityManager.NetworkCallback#onLost(android.net.Network)):
  loss is scoped to the registered request/default network, not proof that this
  application's media connection failed.
- [Quinn 0.11.12 Endpoint](https://docs.rs/quinn/0.11.12/quinn/struct.Endpoint.html):
  closing an endpoint ends connections; socket rebinding is a separate operation
  and does not guarantee incoming connections survive. This increment does not rebind.
