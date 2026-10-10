# ADR 0022: Bounded DLNA GENA event subscriptions

- Status: Accepted
- Date: 2026-10-10
- Scope: M3 DLNA/UPnP eventing

## Decision

Add a bounded `GenaRegistry` to `frameark-dlna` and route `SUBSCRIBE` and
`UNSUBSCRIBE` through `MediaRendererHttpService`. New subscriptions require
an HTTP(S) callback, `NT: upnp:event`, and a bounded `TIMEOUT`; renewals require
the existing SID. The registry emits deterministic local SIDs, monotonically
increasing event numbers, and XML property-set bodies for AVTransport and
RenderingControl state changes. Pending events are capped and drained by the
caller, which performs the actual callback HTTP request.

## Security and evidence

Service types, callback schemes, SID shape, timeout range, property names,
values, subscription count, and queued event size are bounded. Unknown SIDs,
bad callbacks, duplicate subscription headers, invalid leases, and unsupported
services fail closed. Tests cover initial events, mutation events, renewals,
unsubscribe, sequence numbers, service scoping, and rejection cases.

The registry records lease durations but intentionally does not own a clock or
socket. A daemon must expire leases, enforce callback deadlines, and avoid
logging private callback URLs before claiming interoperable GENA support.
