# RTSP accepted-connection I/O verification — 2026-10-15

Scope: [ADR 0134](../../adr/0134-airplay-rtsp-io.md), RAOP and mirror Rust TCP
adapters. Local execution used Windows loopback, not Apple devices or physical
media output. Base: `fe0958a24f9b2428ca335658fa7d06813360c053` (PR #139).

## Reproduction before correction

New real-TCP fixtures explicitly made the accepted socket nonblocking, as can
happen when accepting from the runtime's nonblocking listener on Windows. The
client sent an OPTIONS header and body in three pieces, delayed by 40 ms each.
Both RAOP and mirror returned `Io(WouldBlock)` instead of the expected reply.
This reproduces the failure without relying on a scheduler race or Windows-only
test behavior. The earlier intermittent assertion remains recorded in the
[image evidence](../2026-10-15-dlna-images/README.md).

A second fixture used a blocking accepted socket, an 80 ms connection budget,
and eight body bytes sent 35 ms apart. The unchanged implementation accepted the
request after approximately 290 ms instead of returning `Io(TimedOut)`. Per-read
socket timeouts were being renewed by each successful read.

These were failing regressions before implementation changes; they were not
repaired by increasing test deadlines or removing assertions.

## Corrected behavior

Ten new tests cover the shared I/O implementation and both adapters:

- delayed and fragmented requests on explicitly nonblocking accepted sockets;
- slow-body expiry under one original connection deadline;
- runtime expiry followed by a healthy client and restored blocking accept;
- zero, over-60-second and maximum-duration configuration rejection;
- malformed, duplicate, oversized and truncated body framing without dispatch;
- exact 64 KiB messages with 32 KiB bodies accepted over real TCP;
- partial writes, WouldBlock/Interrupted retry, terminal errors and WriteZero;
- no renewed response/retry budget, and rejection of success after expiry;
- validated length arithmetic and response limits before I/O/allocation.

The malformed/incomplete-request cases check that both sessions remain Idle.
Response-write tests use a deterministic writer to force partial writes and
errors; they do not claim a physical-network congestion benchmark. An already
dispatched request can still change session state before a response write fails.

## Local verification

All commands ran in the focused branch with the shared Windows Cargo target:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked --quiet
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --workspace --all-features --no-deps --locked
python -m unittest discover -s scripts -p test_*.py
```

Results: 294 Rust tests passed, including 62 AirPlay tests; 23 Python tests passed;
Clippy, formatting and rustdoc passed. The opt-in physical multicast test remains
ignored and is not counted as physical discovery evidence.

After the workspace run, the original RAOP and mirror
`rtsp_runtime_is_time_and_request_bounded` tests each passed 100 consecutive runs.
All ten new `rtsp_io` tests passed 20 consecutive runs. These repeated checks
support the deterministic correction; they are not a latency, soak or real-time
guarantee. Cross-platform CI and the normal security/review gates are still
required before merge.

## Unchanged acceptance boundaries

No Android UI, JNI, codec, dependency, secret, production signature or release
artifact changes. No owner device or emulator was operated for this increment.
The existing `0.1.0-preview-65e5820` APK pair remains the owner's test package;
it does not expose these caller-owned AirPlay adapters.

This increment does not add persistent RTSP, pairing, Apple authentication,
RAOP speaker output, mirror decoder output, physical-device compatibility or
long-run synchronization evidence. M4/M5 acceptance remains open. No DRM/HDCP
bypass or certified protocol behavior is claimed.
