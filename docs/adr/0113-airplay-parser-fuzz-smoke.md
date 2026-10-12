# ADR 0113: Deterministic AirPlay parser fuzz smoke

- Status: Accepted
- Date: 2026-10-12
- Milestone: M4/M5 parser hardening

## Decision

The AirPlay crate runs a deterministic bounded arbitrary-byte smoke corpus
against RTSP, RTP, HLS, XML plist, and binary plist parsers. Each parser must
return a bounded success or an explicit error; the test is not allowed to
panic, allocate an unbounded buffer, or retain the generated input.

## Scope

This is a fast regression guard for pull requests, not a replacement for a
coverage-guided nightly fuzz campaign. Future nightly fuzzing may reuse these
entry points and should promote every discovered crash into a fixed regression
fixture without committing private captures.

## Evidence

The test executes 512 deterministic inputs of at most 2048 bytes through all
five parser boundaries. It complements the malformed, duplicate, truncation,
depth, and size fixtures already in the crate. Apple pairing, encrypted media,
decoder output, and named-device behavior remain outside this parser test.
