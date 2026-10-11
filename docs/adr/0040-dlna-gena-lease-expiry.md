# ADR 0040: Monotonic GENA lease expiry

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP event subscriptions

## Decision

Track each GENA subscription with a monotonic `Instant` deadline in addition to
the UPnP timeout value returned to the client. Renewals replace that deadline.
The registry exposes deterministic `expire_at` and runtime `expire` helpers,
and removes expired subscriptions before publishing events. The HTTP
MediaRenderer handler also reaps expired leases at the start of every request,
so a daemon does not need a dedicated timer to prevent stale callback entries
from consuming the bounded subscription table.

## Security and lifecycle

Lease deadlines are not derived from wall-clock timestamps supplied by a peer.
Expired callbacks receive no further `NOTIFY` events, and the bounded table can
therefore recover from abandoned clients without retaining callback URLs
indefinitely. Callback connection retries, HTTPS/TLS, and callback health
telemetry remain caller-owned responsibilities.

## Evidence and limits

Unit coverage verifies deterministic expiry, renewal, publish-time cleanup, and
the existing subscription bounds. This does not claim interoperability with a
named DLNA controller or a production scheduler; the protocol label remains
Experimental.
