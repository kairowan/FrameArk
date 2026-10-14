# ADR-0075: Drive bounded Android receiver recovery from connectivity callbacks

## Status

Automatic restart superseded by [ADR 0126](0126-retain-receiver-on-network-notifications.md).
The observer remains a coalesced endpoint-view refresh, not a session-resume mechanism.

## Context

The Android receiver already had a Rust-owned `ReceiverSessionCoordinator.recover()`
seam, but no platform callback connected it to Wi-Fi or route changes. A stale
ephemeral FANP listener could therefore remain after a short disconnect or
interface change. Duplicating network/session state in Kotlin would create a
second protocol state machine.

## Decision

`ReceiverNetworkObserver` is a small lifecycle adapter with an injected
register/unregister boundary for tests and platform implementations. The
Android service maps `ConnectivityManager.registerDefaultNetworkCallback` events
(`onAvailable`, `onLost`, `onCapabilitiesChanged`, and
`onLinkPropertiesChanged`) into one coalesced recovery notification. Recovery
work is posted to the main service handler, then calls the existing Rust-owned
`ReceiverSessionCoordinator.recover()` path. The observer is registered only
after a session is running and is unregistered on stop/destroy.

The observer has no device, endpoint, transport, or media state. One event is
coalesced until the service marks the recovery attempt complete; a callback
failure releases the debounce so a later event can retry. Registration failure
is logged and leaves the session running without automatic recovery rather than
inventing a fallback network implementation.

## Consequences

- Short Android network changes can rebuild the bounded native endpoint and
  playback loop without Kotlin duplicating FANP state.
- Callback storms cannot start overlapping recovery attempts.
- Recovery remains best effort; physical network-switch and long-run evidence
  still needs Android hardware and is not implied by these unit tests.
- The observer does not authenticate senders, persist trust, or guarantee
  seamless playback across an interface migration.
