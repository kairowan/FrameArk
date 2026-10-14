# ADR 0130: Service-scoped GENA and bounded LastChange snapshots

- Status: Accepted, Experimental
- Date: 2026-10-14

## Problem and decision

The renderer routed SUBSCRIBE/UNSUBSCRIBE to SOAP control URLs, generated local
counter SIDs, started events at 1 and broadcast another initial event to existing
subscribers whenever a new subscriber joined. AVTransport/RenderingControl
events exposed plain properties instead of their LastChange XML envelope.

Subscriptions now use each advertised `eventSubURL`. Renewal and unsubscribe
require a live SID for that service; cross-service operations return 412.
Incompatible SID/NT/CALLBACK combinations and request bodies return 400.
SIDs use UUID v4 randomness from the already-locked `ring` system RNG, with
bounded collision attempts and fail-closed entropy errors. SIDs are identifiers,
not authorization. No new third-party package/version is introduced.

Each new subscriber gets only its own captured initial state at SEQ 0. Later
notifications use the full unsigned 32-bit range and wrap from 4294967295 to 1.
Renewal does not emit another initial. Pending noninitial snapshots are coalesced
per subscriber without consuming sequence numbers until a new queue entry is
allocated. This preserves the latest state and does not overwrite the initial.

AVTransport emits an escaped LastChange XML document using the AVT namespace,
InstanceID 0, acknowledged transport state, URI/metadata, track count and duration.
RenderingControl uses the RCS namespace and Master channel volume. Attribute
escaping preserves tab/newline/carriage-return data across both XML layers.
Position variables are not evented. ConnectionManager emits SourceProtocolInfo,
SinkProtocolInfo and CurrentConnectionIDs directly. The limited SCPD now declares
these event variables correctly; it is **not** a complete service/action schema.

## Bounds, lifetime and delivery

- At most 16 active subscriptions and 32 pending events. Admission reserves one
  initial plus one coalesced update slot per new subscriber; exhaustion returns
  503 before allocating a SID. Churning unsubscribe cannot grow an unbounded queue.
- Initial SEQ 0 snapshots survive unsubscribe/lease expiry to satisfy the UDA
  initial-event rule. Inactive noninitial updates are discarded. The queue must
  be drained; otherwise retained initials eventually prevent new subscriptions.
- Decoded URI metadata is capped at 4 KiB and XML 1.0 text is validated before
  output dispatch. SOAP bodies remain capped at 16 KiB. Generated property values
  are at most 64 KiB; escaped event bodies are at most 124 KiB, within the 128 KiB
  complete HTTP bound. This accounts for both metadata copies and double escaping.
- Drain only after the successful subscription response has been closed. The TCP
  adapter drops the newly allocated lease and initial event if response encoding
  or writing fails. Custom users of `handle` own equivalent response lifecycle.
- HTTP callbacks accept default port 80, bracketed IPv6 and query-only targets;
  credential-bearing, fragmented and malformed authorities are rejected. HTTPS
  still needs the explicit caller-injected TLS connector.

The Rust host must authorize and pin callback destinations, enforce network/DNS
policy, bound callback work and implement the service's 200 ms event moderation.
Queue coalescing alone is not timed moderation. A retained initial is not a license
to bypass revocation or security policy. Already-drained events belong to the
caller: delivery order, delayed-event validation, retry budgets and callback
failure handling remain its responsibility. The convenience callback client has
no SSRF authorization or bounded DNS resolver; do not expose it directly to an
untrusted network. No background thread, callback daemon or new listener starts.

## Migration, evidence and remaining work

This is a pre-alpha API/wire correction: clients must use the advertised event
URL, accept random SIDs and SEQ 0, and parse LastChange. `set_capabilities` now
returns a validation Result and queues a ConnectionManager update. Generic
GenaRegistry callers must supply a complete first snapshot themselves.

Thirteen new Rust tests cover route/service isolation, malformed headers and
URLs, UUID shape/isolation, sequence wrap, 16-subscriber saturation, initial-event
retention/admission, coalescing, metadata/escaping limits, capability updates,
default callback ports/targets, real loopback subscription-to-NOTIFY delivery and
response-write failure cleanup. All 67 DLNA tests pass locally. An original RCS
fixture is byte-compared with Rust output and independently parsed through both
XML layers with Python ElementTree in the existing repository test job.

These are protocol fixtures, not a decoder or named-controller certification.
Complete SCPD action arguments/mandatory actions, timed event delivery, shared
Session host integration, asynchronous decoder feedback, actual Android output,
multicast lifecycle and the named-controller/two-hour M3 matrix remain open.
M3 remains Partial/Experimental. Rollback is a normal reviewed revert; there is
no persisted schema or production signing change.

## Primary specifications

- [UPnP Device Architecture 1.0, section 4](https://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v1.0-20080424.pdf)
- [AVTransport:1 eventing and LastChange schema](https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf)
- [RenderingControl:1 eventing and RCS schema](https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf)
- [ConnectionManager:1 evented variables](https://upnp.org/specs/av/UPnP-av-ConnectionManager-v1-Service.pdf)
