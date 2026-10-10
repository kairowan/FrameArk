# ADR 0013: M1 fixture sender CLI

- Status: Accepted
- Date: 2026-10-10
- Scope: M1 Native protocol lab tooling

## Decision

Add `tools/frameark-cli` as a small dependency-light reference sender for
encoded fixture access units. The `send` command performs pinned temporary
FANP pairing, opens the existing bounded unidirectional media stream, wraps
caller-supplied H.264 and/or Opus/AAC bytes in FAM1 packets, and sends a fixed
number of frames with a bounded interval. Fixture sizes, frame counts, ports,
and intervals are validated before network activity. Pairing failures and
fixture errors are reported without printing pairing codes or packet bytes.

The CLI deliberately does not contain a software codec, synthesize a claimed
valid H.264/Opus/AAC stream, or implement a second control-plane state machine.
It is a lab/reference sender until Offer/Start/Stop orchestration and real
codec fixtures are added.

## Verification

The workspace builds and tests the binary with the same locked Rust toolchain
as the core. Unit tests cover option bounds and missing fixture handling; the
transport/native crates continue to cover pinned QUIC and FAM1 loopback.
