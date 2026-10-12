# ADR 0110: Redacted device compatibility evidence template

- Status: Accepted
- Date: 2026-10-12
- Scope: M0–M5 compatibility and release evidence

## Decision

Device and controller verification uses a checked-in, redaction-first record
template. A record names the exact receiver and sender builds, OS/device,
network topology, codec/mode, expected and observed behavior, duration,
recovery result, diagnostics reference, limitations, and review state. The
template explicitly distinguishes `Unavailable` from `Pass` so a missing
hardware lab cannot be represented as an interoperability claim.

## Privacy and security

Records may contain only bounded support codes and counters. Media, packet
payloads, credentials, pairing codes, raw keys, certificates, private
addresses, persistent device identifiers, and unsanitized captures are
prohibited. AirPlay and security records require a second-maintainer or issue
review before being treated as release evidence.

## Consequence

The repository can state precisely which M0–M5 exit gates are evidenced while
keeping hardware-dependent gates visibly open. Loopback and unit tests remain
valuable implementation evidence but cannot be promoted to named-device or
long-duration compatibility claims.
