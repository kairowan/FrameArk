# ADR 0114: Deterministic DLNA parser fuzz smoke

- Status: Accepted
- Date: 2026-10-12
- Milestone: M3 parser hardening

## Decision

The DLNA crate runs a deterministic bounded arbitrary-byte smoke corpus against
HTTP request/response, SSDP, and media-range parsers. Each boundary must return
bounded success or an explicit error without panicking or retaining generated
input.

## Scope

This fast pull-request guard complements the malformed, duplicate, truncation,
range, XML, and size fixtures already present. It is not a replacement for a
coverage-guided nightly fuzz campaign; any future crash must become a redacted
regression fixture.

## Evidence

The test executes 512 deterministic inputs of at most 2047 bytes through all
four parser boundaries and retains no input after each iteration. Named
Android/iOS/desktop controller runs and the two-hour playback/Seek gate remain
separate M3 requirements.
