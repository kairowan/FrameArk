# ADR 0124: explicit Android Sender discovery without automatic trust

- Status: Accepted for Experimental use
- Scope: M1 discovery-to-pairing UX; not a physical-LAN or M2 exit gate

## Decision and ownership

Use the existing Rust mDNS adapter and FANP pairing/session path. The new
`frameark-native::discovery` module defines the FANP-specific bootstrap hints
and candidate cache; it depends downward on discovery/transport. No new
third-party crate is introduced. Kotlin only owns the foreground Wi-Fi multicast
lock, list rendering and user confirmation. No Android-specific protocol or
second session state machine is introduced.

Receiver publishes its ephemeral **public** TLS certificate as small, versioned
TXT chunks. Sender lists bounded numeric local endpoints, not a peer-controlled
name that could imitate a trusted UI. Selection fills the existing ticket form,
clears the PIN/confirmation and shows the entire TLS SHA-256. Users must compare
address and all 64 fingerprint characters against Receiver, then confirm and
enter PIN. No socket connection is initiated by discovery/selection. The existing
fixed `frameark.local` name, pinned DER certificate and TLS/PIN negotiation remain.
Discovery never enrolls trust, supplies a PIN, or satisfies proof-required mode.

## Experimental DNS-SD extension

Under `_frameark._udp.local.`, require `proto=fanp1`, `ver=0.1`, `tls=pin` and:

- `boot=cert1`: this bootstrap-hint version, not a new FANP transport version.
- `certs=N`: canonical one-digit count 1..5.
- `cert0` through `certN-1`: lowercase hex DER, 240 characters per nonfinal
  chunk; final chunk is nonempty, even length and at most 240 characters.
- Reassembled DER is 1..512 bytes. Each TXT key/value string is at most 255
  bytes; all TXT strings including length bytes total at most 1300 bytes.

Keys are case-insensitive; case-colliding duplicates and extra reserved `cert*`
keys are rejected. Up to 16 properties are allowed; other bounded optional
properties are ignored. Malformed/missing chunks, unsupported versions,
zero port, overlong service fullname, and more than 16 addresses reject the
candidate. Hostname text is never resolved or displayed. Only RFC1918/link-local
IPv4 passes the existing ticket parser; IPv6/public/loopback/multicast do not.
DER validity is finally enforced by TLS, not claimed by the discovery parser.

These limits follow DNS-SD's 255-byte per-string syntax; see
[RFC 6763 sections 6 and 15](https://www.rfc-editor.org/rfc/rfc6763.html).
The certificate remains unauthenticated until the user's out-of-band comparison.
An attacker can advertise their own certificate or cause a discovery denial of
service; a matching service name, address or PIN prompt is not authentication.
Do not weaken fingerprint comparison based on these hints.

## Bounds, lifetime, cleanup and diagnostics

One Rust browser generation runs for 15 seconds, at most 4096 consumed events.
The application cache retains at most eight services/four addresses each
(32 candidate tickets), deduplicates identical tickets, expires each entry
30 seconds after its last resolution, and removes goodbye/invalid updates.
The mdns-sd backend retains its own bounded event channel and internal DNS cache;
this increment does not establish a strict byte cap on that backend cache or
claim hostile-LAN flood resistance. Scans are finite; long-run/flood tests remain
open before any Stable claim. A selected ticket is a user-owned form snapshot,
not refreshed behind an already confirmed fingerprint. Stale pins fail TLS.

JNI start/status/tickets/stop are generation-scoped. Start returns immediately;
one worker does socket work, polling cancellation every 100 ms between backend
operations. Backend shutdown has a two-second acknowledgment budget. Stop
invalidates reads immediately and clears the cache; no UI-thread join. A new
scan during cleanup returns busy rather than spawning overlapping workers.
No native exception text, remote names, certificates or endpoints are logged.
UI reports scanning/completed/failed/stopped and candidate count only; selected
endpoint/fingerprint is visible solely in the screenshot-protected local form.

The multicast lock follows Android's
[MulticastLock lifecycle](https://developer.android.com/reference/android/net/wifi/WifiManager.MulticastLock).
It is held only during a user-requested foreground scan; completion, failure,
cancel, pause and destruction release it. UI polling continues after completion
only while unexpired candidates exist. Returning to the foreground does not
scan/connect automatically. There is no clipboard/intent import, persisted
candidate history or new secret.

## Compatibility, verification and rollback

Executed checks, runtime boundaries and the empty-form screenshot are recorded
in the [2026-10-14 evidence](../evidence/2026-10-14-sender-discovery/README.md).

Older receivers without `boot=cert1`, certificates over 512 bytes, incompatible
versions, and filtered multicast are absent from the automatic list. Existing
manual tickets (up to 4096 DER bytes) remain supported; pure IPv6 still is not.
Transport ALPN/messages, session lifecycle and ABI version are unchanged; JNI
methods are additive and matching APK/native builds remain required.

Tests cover certificate/chunk boundaries, malformed/version/downgrade inputs,
address filtering, cache capacity/expiry/goodbye/change, generation cancellation,
and Android UI/lock cleanup. The opt-in `discovery_lan` integration test publishes
and browses through separate actual mDNS daemons, checks the known certificate,
performs real private-IPv4 QUIC/PIN, negotiates 1080p30 and reaches Start/status/
Stop with an instrumented backend. It does **not** decode video or stand in for
two-device Wi-Fi/hardware acceptance. Physical operator instructions and evidence
remain separate. Remove the UI/`boot` properties to roll back; manual pairing and
existing transport remain available. QR, IPv6, identity-proof Sender, network
switch recovery and hardware performance remain follow-up work.
