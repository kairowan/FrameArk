# ADR 0134: One absolute I/O budget for accepted RTSP connections

- Status: Accepted, Experimental
- Date: 2026-10-15
- Scope: M4/M5 RAOP and mirror TCP correctness

## Evidence and decision

The image increment recorded an intermittent Windows mirror runtime assertion.
Inspection found that `serve_for` makes its listener nonblocking, while accepted
connections assumed per-call socket timeouts made `read` blocking. Windows accept
inherits socket properties. A new delayed/split-request fixture explicitly marks
the accepted socket nonblocking on every OS: both unchanged adapters returned
`Io(WouldBlock)` instead of a valid response. A second fixture showed that body
fragments arriving within each per-read timeout could extend the total operation.

Move accepted I/O into one private Rust module used by both adapters. Explicitly
select nonblocking mode and retry only WouldBlock/Interrupted, sleeping at most
one millisecond between attempts. Check the original deadline before and after
I/O and after parsing. Share it with response writes, preserving partial writes;
terminal OS errors and zero writes fail. Do not add a second flush budget for TCP.
`serve_for` passes its absolute outer deadline, capped by the configured connection
budget, so scheduling delay between phases cannot recreate an expired budget.

Incremental framing scans newly received header bytes with a three-byte overlap,
uses the existing header grammar, then checks Content-Length before reading the
body. Checked arithmetic and existing 64 KiB message/32 KiB body caps avoid overflow
or truncating excess reads into an apparently valid prefix. A complete message
exactly at the limit is accepted. Missing Content-Length means no network-framed
body. Premature EOF, already-buffered trailing bytes, duplicate/invalid lengths
and oversized declarations fail before session dispatch. No second/pipelined
request is processed. The standalone complete-message parser's grammar is unchanged.

## Compatibility and lifetime

- Public `bind` signatures retain their existing timeout parameter name, but it
  now means the whole accepted connection's read/dispatch/write budget. Values
  must be nonzero and <= `MAX_RTSP_CONNECTION_DURATION` (60 seconds); callers with
  larger per-read timeouts must choose a bounded total budget. No default expands.
- Budget expiry consistently returns `AirplayError::Io(TimedOut)`. Terminal OS
  errors retain their error kind. Fixed diagnostic text now says "network" rather
  than incorrectly labeling every I/O failure "RTP"; no peer data is included.
- `serve_once` may still wait indefinitely for accept; this budget starts only
  after admission. Caller-supplied authorization callbacks are not preempted.
  Use the bounded `serve_for` scheduler for bounded accept windows. Its returned
  count remains accepted attempts, not a count of successful RTSP replies.
- One-request connections close on return. The runtime restores listener blocking
  mode; an incomplete/expired request does not poison the next client or mutate
  media state. A fully dispatched valid request may mutate session state before a
  write fails; caller-owned teardown/recovery is still required. This is not a
  transactional guarantee of peer receipt, parallel scheduling or hard real time.
- Peer authorization still runs before bytes reach the parser on the explicit
  policy entry point. Allow-all convenience methods remain fixture APIs, not
  authenticated Apple endpoints. No pairing, TLS, protected-media handling,
  audio/video decoder or new default network listener is added.

No dependencies, FFI, Android UI, keys or persistence formats change. The previous
Android preview remains the device-test artifact because it does not expose these
caller-owned AirPlay servers. No new APK or Apple interoperability claim is needed
for this core correction. Rollback is a reviewed revert; do not remove the regression
cases or enlarge their deadlines to conceal the original fault.

## Verification

The [verification record](../evidence/2026-10-15-rtsp-io/README.md) covers failing
pre-fix reproducers, shared framing/retry tests, real delayed/split/maximum-sized
TCP requests on both adapters, incomplete-body timeout, a subsequent healthy
request, restored listener behavior and repeated original runtime assertions.
Cross-platform CI is required before merge. This is not Apple-device, physical
decoder, long-run synchronization or M4/M5 milestone acceptance.

## Primary references

- [Windows accept socket inheritance](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept)
- [Rust TcpStream nonblocking behavior](https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.set_nonblocking)
- [Rust socket timeout semantics](https://doc.rust-lang.org/std/net/struct.TcpStream.html#method.set_read_timeout)

This supersedes the per-operation timeout implementation described in
[ADR 0063](0063-airplay-mirror-bounded-runtime.md) and
[ADR 0064](0064-airplay-raop-bounded-runtime.md), not their protocol scope.
