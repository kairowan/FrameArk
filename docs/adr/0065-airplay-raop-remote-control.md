# ADR 0065: Bounded RAOP remote-control parameters

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay/RAOP audio control boundary

## Decision

Extend the RAOP session with a bounded `text/parameters` contract for
`SET_PARAMETER` and `GET_PARAMETER`. The receiver accepts volume in the
`-144..0` dB range, monotonic sample progress (`start/current/end`), and a
small allowlisted set of UTF-8 text metadata fields. The state is retained
per session and can be queried through typed Rust accessors; unknown parameter
names are rejected on writes and ignored on reads.

The body is limited to 4 KiB, metadata values to 512 bytes, and metadata
fields to eight. This is a control-plane boundary only: it does not claim
Apple DMAP tag compatibility, FairPlay authentication, encrypted audio, or a
platform volume/output implementation.

## Verification

The AirPlay suite covers a complete ANNOUNCE → SETUP → RECORD path, valid
volume/progress/title/artist updates, deterministic GET responses, missing
content-type rejection, and out-of-range volume rejection. Device and
long-running synchronization evidence remain required for stronger labels.
