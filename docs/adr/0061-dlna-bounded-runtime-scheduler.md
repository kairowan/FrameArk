# ADR 0061: DLNA bounded runtime scheduler

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA MediaRenderer TCP boundary

## Decision

Expose `MediaRendererTcpServer::serve_for` as a caller-owned runtime
primitive. It temporarily switches the listener to non-blocking mode, applies
an explicit wall-clock budget (at most 60 seconds), caps accepted requests by
the existing batch limit, sleeps briefly while idle, isolates malformed client
connections, and restores blocking mode before returning.

The API does not create unbounded threads, own a process-wide scheduler, or
claim concurrent production throughput. Existing `serve_once` and
`serve_requests` behavior remains unchanged for hosts that provide their own
loop.

## Verification

DLNA tests cover zero/oversized runtime budgets and a real loopback request
served by the time/request-bounded helper. Formatting, Clippy, workspace tests,
and documentation remain CI requirements.
