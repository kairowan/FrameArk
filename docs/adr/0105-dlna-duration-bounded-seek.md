# ADR 0105: Bound DLNA relative seeks by known duration

- Status: Accepted
- Date: 2026-10-12
- Milestone: M3

## Context

The bounded MediaRenderer already parses `REL_TIME` seeks and reports a
metadata-derived duration when one is available. Accepting a position past
that duration leaves the protocol state inconsistent and forces every caller
to discover the error later during playback.

## Decision

Parse the target before mutating `MediaRendererState`. When metadata supplies a
known duration, reject a target strictly greater than that duration with the
UPnP `711` seek-target fault. The state remains at its previous position. If
duration is unknown, retain the existing bounded time parser and allow the
caller-owned media backend to decide whether the target is seekable.

## Consequences

Known-duration control points now have deterministic state-transition behavior
and cannot move past the advertised end. This is a protocol/state guard, not a
claim that every remote URL supports seeking; production connector and decoder
behavior remain caller-owned.

## Evidence

`renderer_executes_avtransport_and_volume_lifecycle` verifies an out-of-range
target is rejected without changing position. Repository compatibility and
status documents record the boundary.
