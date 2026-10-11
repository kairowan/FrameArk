# ADR 0039: Native CLI discovery and lifecycle commands

- Status: Accepted
- Date: 2026-10-11
- Scope: M1 FrameArk Native Protocol operator workflow

## Decision

Extend `frameark-cli` with bounded `discover`, `connect`, `status`, and `end`
commands. Discovery browses `_frameark._udp.local.` for a finite event window.
The lifecycle commands read an explicit server address, pinned certificate, and
six-digit pairing code, create one temporary FANP connection, perform the
requested operation, print a redacted state summary, and close the connection.

No command stores credentials, creates persistent trust, retries indefinitely,
or labels a successful pairing as media playback. The existing `send` command
remains the explicit encoded-fixture path.

## Evidence and limits

Unit tests cover bounded timeout/event options, required pairing inputs, and
command usage. The Rust transport/native loopback suite covers the underlying
pairing and Offer/Start/Status/Stop semantics. Live mDNS, Android hardware,
reconnect, and named receiver matrices remain unverified and Experimental.
