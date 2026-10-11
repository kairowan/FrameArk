# ADR 0091: M0 reference-runtime smoke check

- Status: Accepted
- Scope: M0 CI and release evidence

## Context

Workspace compilation and unit tests did not prove that a packaged third-party
receiver binary can start a real QUIC listener. M0 requires runtime evidence
in addition to library compilation and Android packaging checks.

## Decision

The required CI gate now builds `framearkd` on Ubuntu and starts it with an
ephemeral loopback UDP port, a bounded localhost server name, and a fixed test
pairing code. The process must remain alive for the bounded eight-second smoke
window; the job then terminates it deliberately. No network peer, media, or
secret is recorded.

This is a Linux reference-runtime check, not proof of Windows/macOS support or
hardware playback. Those platform load and compatibility records remain
separate M0/M1 exit evidence.
