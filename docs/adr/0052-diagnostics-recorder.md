# ADR 0052: Bounded diagnostics recorder

- Status: Accepted
- Date: 2026-10-11
- Scope: M0/M1 cross-platform lifecycle diagnostics

## Decision

Add `frameark-diagnostics` as the platform-neutral diagnostics sink shared by
Rust services, Android, desktop, and CLI adapters. It retains a bounded in-
memory ring of caller-supplied lifecycle and error summaries, normalizes
control characters, truncates fields, and renders deterministic tab-separated
support text. Session identifiers are deliberately omitted from transition
records.

The recorder does not write files, upload telemetry, retain packets, or infer
secrets. Platform code may persist or present a snapshot only after applying
its own user-consent and secure-storage policy; callers remain responsible for
providing redaction-safe messages.

## Verification

Unit tests cover capacity bounds, oldest-record eviction, text sanitization,
deterministic rendering, transition identity omission, and error severity
mapping.
