# ADR 0040: Monotonic GENA lease expiry

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP event subscriptions

## Decision

Track each GENA subscription with a monotonic `Instant` deadline in addition to
the UPnP timeout value returned to the client. Renewals replace that deadline.
The registry exposes deterministic `expire_at` and runtime `expire` helpers,
and removes expired subscriptions before registering, renewing, or publishing.
Renewal at or after the deadline fails rather than reviving a stale SID. The
HTTP MediaRenderer handler reaps leases at request and event-drain boundaries;
pending events for expired/unsubscribed SIDs are discarded. A host timer may
call `expire_subscriptions` for idle cleanup and use its removal count in
redacted diagnostics; without a timer idle records remain bounded until the
next operation.

## Security and lifecycle

Lease deadlines are not derived from wall-clock timestamps supplied by a peer.
No new events are generated or drained for expired callbacks. Events already
transferred to the caller cannot be recalled, so a caller delaying delivery
must re-check its authorization and lease policy. The bounded table recovers
expired capacity on registration. Callback retries, HTTPS/TLS, and callback
health telemetry remain caller-owned responsibilities.

## Evidence and limits

Unit coverage verifies exact-deadline expiry, renewal, rejection of stale SID
renewal, capacity reclamation, pending-event cleanup, and HTTP 412 behavior.
This does not claim interoperability with a
named DLNA controller or a production scheduler; the protocol label remains
Experimental.
