# ADR 0129: Acknowledge DLNA output before committing control state

- Status: Accepted, Experimental
- Date: 2026-10-14

## Problem

The MediaRenderer handler previously acknowledged SetAVTransportURI, Play,
Pause, Stop, Seek and SetVolume by mutating a Rust struct. No output operation
was required. This is useful in a state-model fixture, but a socket consumer
could mistakenly report successful playback without an attached output.

## Decision

`MediaRendererHttpService::new` is now unbound and fails valid output mutations
with a redacted SOAP 501 / HTTP 500. Queries, descriptions, explicitly registered
resources and subscriptions remain available. `with_backend` takes ownership
of one `MediaPlaybackBackend`. Rust validates the command before dispatch,
dispatches exactly once, and commits state and success events only after the
backend acknowledges it. Master volume is limited to this profile's 0..100;
time arithmetic is checked before dispatch and invalid metadata durations
remain unknown instead of becoming wrapped values.

The typed, borrowed `PlaybackCommand` contains no platform object and has no
automatic Debug representation containing a URL or metadata. No command queue,
retry, fallback, callback, new network listener or dependency is introduced.
Existing TCP paths use the same handler and therefore the same output gate.

The backend is synchronous and Send; the host chooses its thread. It must bound
all I/O/queue waits. Acceptance is not evidence that a frame has been displayed.
Unavailable/Unsupported/Rejected mean no lasting output change, so the prior
Rust state and event sequence remain intact. Partial or uncertain output failure
must return Failed: Rust clears URI/metadata/position/duration, reports no media,
emits a terminal transport event, shuts down and detaches the backend. Future
mutations cannot silently revive it. Service drop and constructor rejection
also call bounded, idempotent shutdown. A blocked or panicking implementation
cannot be forcibly recovered by this synchronous trait.

`PlaybackDispatchReport` exposes only saturating attempts, acknowledgments,
nonfatal rejections, fatal failures and a fixed error classification. No URI,
metadata, media, platform exception, credentials or peer address is included.
Diagnostics are command acceptance counters, not decoded/rendered-frame counts.

## Ownership and migration

Protocol validation, control state and events remain in Rust. A future Rust
host must own source authorization, pinned DNS/address policy, redirects,
timeouts, resource budgets and the shared `frameark-core::Session` lifecycle.
Platform code implements only OS demux/decode/output and lifecycle operations;
it must not independently fetch an unvetted URL or invent a session state machine.
URI syntax validation alone is not authorization or an SSRF defense.

This is an intentional pre-alpha API/behavior change. Callers that need output
must use `with_backend`; no production no-op backend is provided. `state_mut`
is removed to prevent bypassing acknowledgments. State-model tests explicitly
install a test-only acknowledging backend. The state snapshot represents
accepted commands; asynchronous progress, completion/error reconciliation and
decoder-derived duration need a later generation-bound feedback API.

## Evidence and remaining work

Eight focused regression tests cover all six output commands, three nonfatal
error categories for each command, fatal failure for each command, no-backend
behavior, invalid arguments, volume endpoints, event sequence preservation,
constructor/drop cleanup, time bounds, counter saturation and real loopback TCP dispatch.
The full crate suite contains 53 passing tests. These use an instrumented output
fixture, not a decoder, Android, or a named DLNA controller.

The new event assertions also exposed an existing interoperability defect:
GENA routes currently use control URLs rather than the advertised event URLs.
The fixture explicitly uses that legacy route. Correct routing and SID service
scoping must be repaired separately before live controller integration. Full
LastChange event encoding, complete SCPD declarations, asynchronous feedback,
safe media fetching, Android output, multicast lifecycle and the named-client /
two-hour M3 acceptance matrix remain open. No M3 exit gate is completed here.

## Primary references

- [UPnP AVTransport:1, actions and common errors](https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf)
- [UPnP RenderingControl:1](https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf)
- [OCF MediaServer:1 and MediaRenderer:1 specifications](https://openconnectivity.org/developer/specifications/upnp-resources/upnp/mediaserver1-and-mediarenderer1/)
