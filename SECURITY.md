# Security policy

FrameArk processes untrusted network traffic and media. Please report vulnerabilities privately and do not open a public issue containing exploit details, credentials, private packet captures, or user media.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting or a private draft Security Advisory in the `kairowan/FrameArk` repository. Include, when safe:

- affected commit, component, protocol, and platform;
- prerequisites and realistic impact;
- minimal reproduction steps or a sanitized fixture;
- whether secrets, identity, remote input, file access, or memory safety are involved;
- any suggested mitigation.

If private vulnerability reporting is unavailable, open a public issue containing only a request for a private contact channel—do not include vulnerability details.

The project aims to acknowledge complete reports within 72 hours and provide an initial triage within 14 days. These are targets rather than a service-level guarantee for a volunteer project.

## Supported versions

FrameArk is currently pre-alpha and has no supported release line. Security fixes will target the active development branch until the first published release. A supported-version table will be added before 1.0.

## Security scope

Relevant reports include:

- unauthenticated or authorization-bypassing sessions;
- pairing, trust, replay, downgrade, or identity failures;
- parser crashes or unbounded CPU, memory, storage, queue, or recursion use;
- unsafe media URL fetching or management API access;
- sensitive logs, diagnostic bundles, packet captures, keys, or tokens;
- remote input, clipboard, file-transfer, or FFI memory-safety defects;
- build, release, dependency, or artifact supply-chain compromise.

The absence of FairPlay, Widevine, HDCP, certified Cast behavior, or universal Miracast support is not a security vulnerability. FrameArk does not accept requests to bypass DRM or device certification.

The current FANP transport is experimental. It uses QUIC/TLS with an
ephemeral self-signed certificate pinned for the caller's session and a
process-local six-digit pairing code. It does not create persistent device
trust, protect a remembered identity, or expose an internet relay. Do not log,
persist, or reuse pairing codes; report any certificate-verification,
downgrade, replay, or code-disclosure issue privately.

The experimental `frameark-trust` registry accepts only bounded lowercase
SHA-256 public-key fingerprints, rejects silent key changes for a known device,
and requires explicit revocation before replacement. Its `FTR1` snapshot is
only a bounded serialization format and is not encrypted or authenticated.
`TrustSnapshotStore` can flush a temporary snapshot and replace a caller-owned
path, but it does not establish directory permissions, authenticate the file,
or protect private keys. Platform adapters must place it behind secure storage,
authenticated key exchange, and user approval before adding or restoring a
record.
Do not treat a fingerprint string supplied by an unauthenticated peer as proof
of identity.
`PendingTrustRequests` is an ephemeral, bounded approval queue with monotonic
expiry. It removes a request only after the registry accepts it, explicit
rejection, or expiry; failed capacity or identity-rotation mutations leave the
request available for retry. The queue does not authenticate its inputs and is
never persisted in FTR1, so a transport adapter must verify key possession
before enqueueing a request and must keep its UI binding access-controlled.

The optional FANP proof listener may take a startup snapshot of the Rust trust
registry. After the fresh Ed25519 challenge is verified, an exact device-id and
derived public-key fingerprint match is admitted without another prompt; a
rotated key, renamed device, malformed identity, or missing record falls back
to the bounded explicit-approval queue. Android trust mutations remain
stopped-only, so revocation takes effect on the next receiver lifecycle rather
than racing an active listener. This policy is reconnect admission, not Apple
pairing, certificate pinning, or proof of a private key beyond the verified
challenge.

FANP proof approval request IDs are process-monotonic and never reused on
listener restart. Rust enforces a 30-second monotonic deadline and accepts only
the first decision; a pending-query JNI call is advisory and the actual decision
rechecks expiry. Async waiter cancellation removes the request and event. Stop
cancels handshakes and media waits without granting approval or retaining a
request for a later sender. This is local session lifecycle handling, not a
persistent trust grant.

The Android proof prompt is foreground-only. It displays the claimed ID and
key fingerprint for comparison, never treating a name as proof of identity.
Only explicit button actions authorize the current request; leaving the page
rejects its displayed request. Prompts are not saved or logged, approval ignores
obscured touches, and no automatic trust enrollment occurs. The process-local
proof-mode setting is not a durable security preference.

Capability offers are exchanged only after the pinned TLS handshake. The
current schema requires an exact version match and rejects unknown or duplicate
entries before allocation; it does not silently downgrade or authorize media,
remote input, file transfer, or internet relay.

The CLI management commands accept the certificate and pairing code only for
one bounded invocation. They do not persist them, create a trust record, or
reuse a connection across commands; operators must protect the command line,
fixture paths, and certificate files supplied to the process. `framearkd` is a
reference/fixture receiver with the same temporary pairing boundary; it does
not add persistent authorization, decoder isolation, or a production access
policy and must not be exposed as an internet service.

Authenticated control streams enforce one bounded frame per stream and a total
operation deadline. Cancellation closes the connection rather than reusing
partially read state; pairing codes have redacted debug formatting.

Native media offers are validated against negotiated capabilities and explicit
receiver limits before platform resources are prepared. Failed preparation,
timeouts, cancellation, malformed requests, and receiver drop all attempt
backend reset; the control profile carries no encoded media or remote input.

The experimental DLNA handler bounds HTTP headers, bodies, SOAP argument text,
and XML fields, requires exact `Content-Length`, rejects chunked transfer and
unsupported URL schemes, and never fetches a caller-supplied media URL. Its
fixture media helper caps resource bytes and accepts only one bounded range;
it is not a network URL fetcher. An explicitly registered regular file can be
served through the TCP adapter using one bounded chunk buffer and one validated
range; the file path is canonicalized at registration and is never taken from
the HTTP request. The adapter rejects files that change length before serving,
but persistent authorization, TLS, and format probing remain the daemon's
responsibility. The synchronous TCP adapter adds a
read timeout, complete-request bound, response-size bound, and closes each
connection after one response; it does not provide persistent authorization or
multi-client scheduling. The GENA registry caps subscriptions, leases,
callback schemes, XML property names/values, and pending events. Monotonic
deadlines are enforced on subscription mutations and event publication; the
HTTP service drops expired/unsubscribed pending events before handing them to
callback I/O. A daemon must add authorization policy and re-check leases if it
delays delivery of already-drained events. Idle cleanup is available through
`expire_subscriptions`; no background scheduler is started by the library.
The SSDP multicast helper sends only the bounded standard IPv4 announcement to
the caller's selected UDP socket; it does not join interfaces, schedule bursts,
authenticate discovery peers, or expose a trust boundary.
`GenaEvent::encode_http_notify` validates the callback authority and target and
caps the generated request. `GenaCallbackClient` adds bounded plain-HTTP
connect/read/write timeouts and exact-length response parsing, rejects HTTPS
unless a caller-owned TLS adapter is supplied, and does not provide callback
authentication, retries, or SSRF policy beyond URL/authority
validation. `send_gena_callback_with_connector` accepts a caller-supplied
HTTP/TLS stream and keeps the same bounds, but the connector must perform
certificate validation, DNS policy, timeout setup, and peer authorization;
passing a plain stream for an `https` URL is not secure.

`validate_media_uri_network` is the caller-injected DNS-aware preflight for
future URL fetchers: it resolves a hostname once through a bounded resolver,
caps addresses, rejects multicast/unspecified results, and requires every
address to satisfy a public-only or local-network-only policy. It does not
open sockets, follow redirects, or prevent a later DNS change; fetchers must
pin the validated result or repeat the policy at connection time.

The AirPlay HLS boundary follows the same fail-closed model. Its connector
receives only a validated HTTP(S) authority and origin-form target, while the
crate emits one bounded GET and parses one response. It rejects credentials,
fragments, duplicate headers, unsupported transfer encoding, non-HLS MIME
types, oversized bodies, and non-2xx statuses; it never follows redirects,
resolves DNS, establishes TLS, fetches segments, or caches media. A daemon must
apply DNS rebinding, private-network, certificate, authorization, timeout, and
redirect policy before supplying the stream.

The DLNA `MediaFormatCapabilities` report is validated and bounded before it is
advertised through `ConnectionManager`; it is not a decoder probe or a promise
that a remote URL is safe to fetch. `MediaFormatProbe` only parses bounded
response metadata and matches MIME types against that report. Integrations must
derive entries from the actual platform decoder policy and keep URL fetching,
redirect handling, and authorization outside the protocol parser.

The experimental RAOP session rejects unknown codecs, payload mismatches,
interleaved TCP transport, invalid state transitions, and oversized SDP before
allocating media state. It does not authenticate Apple senders or decrypt
protected audio; callers must not expose the unauthenticated state machine to
untrusted networks or treat it as a FairPlay boundary. XML and binary plist
parsers enforce document, object-table, reference, depth, and scalar-size
bounds; binary plist cycles and unsupported object types are rejected before
allocation of nested values.

The RAOP and mirror RTSP TCP adapters expose a caller-owned peer authorization
hook that runs immediately after `accept` and before request bytes reach the
session state machine. A rejected peer is dropped without mutating negotiated
state. This hook is an integration boundary, not Apple pairing or sender
authentication: deployments must bind it to an authenticated RTSP/pairing
identity and still enforce TLS, rate limits, and network policy.

The RAOP RTP jitter buffer caps retained packets, rejects duplicate and late
sequence numbers, handles 16-bit sequence wrap, and requires the caller to
declare a missing-packet recovery point. It does not conceal loss, authenticate
packets, decrypt payloads, or provide a timing guarantee without a future
clock/retransmission policy.

The Android JNI bridge keeps its Rust session and bounded encoded-media queue in
process-local protected state, carries no key material, caps each submitted
frame at 4 MiB and the queue at 16 frames, and maps ABI/start/stop/media
failures without logging JNI arguments. The `FAMF` poll envelope is not an
authorization boundary or decoder; real network pairing, persistent identity,
and renderer ownership must remain in the Rust transport and Android platform
layers. `JniMediaQueueSink` only accepts samples after the Rust receiver
lifecycle is started; it does not open sockets, authenticate senders, or grant
Android decoder/surface access.

The Android JNI receiver now owns a bounded, ephemeral FANP QUIC listener for
each started lifecycle and exposes only its port, temporary pairing code, and
pinned public certificate to an explicit in-app pairing flow. The listener
closes and joins its worker before the Rust lifecycle reports stopped; it does
not persist trust, log pairing material, or imply that a decoder/surface is
available.
`ReceiverNetworkCoordinator` gates the foreground service on a valid JNI
endpoint snapshot and the service notification may display the port and
temporary pairing code to the local user. The certificate remains in memory;
the coordinator does not copy it into logs, preferences, or notification
text. Best-effort mDNS metadata advertises protocol/capability labels only; it
never advertises the pairing code or certificate.
The Android service also acquires a non-reference-counted Wi-Fi multicast lock
for the active lifecycle when the platform provides one. Acquisition failure is
best effort and never bypasses FANP endpoint authentication; the lock is
released on stop/destroy and is not retained by the Rust layer.
The standby Activity reads the endpoint through the JNI getters only to display
the temporary port and pairing code after an explicit user refresh; it never
displays or persists the certificate.
`ReceiverNetworkObserver` listens only to Android's default connectivity
callback and coalesces route/capability changes into the existing bounded
Rust-owned recovery operation. It has no protocol state and does not authorize
or reconnect a sender. Registration failure disables automatic recovery while
leaving the explicit lifecycle available; callback storms cannot start
overlapping recovery attempts.
`MediaPlaybackPump` and `MediaPlaybackLoop` add only bounded polling and
fixed-rate scheduling. The loop cancels on consumer failure, does not decode,
retain surfaces, or bypass the native lifecycle, and releases its executor on
close.

The AirPlay XML Property List boundary caps document size, nesting, entries,
scalar/data fields, duplicate keys, and base64 decoding. It is not a parser for
binary plists or a FairPlay key container; callers must keep pairing secrets out
of diagnostics and only pass authenticated metadata into future crypto code.

The mirror video contract caps access units and NAL counts, rejects forbidden
H.264/H.265 headers, malformed length prefixes, unsupported orientation values,
and unbounded A/V offsets before platform allocation. Its H.265 RTP support is
only a bounded single-NAL/AP/FU assembly boundary. It does not authenticate,
decrypt, decode, or render incoming video; those network and platform layers
must remain behind the receiver's authorization boundary.

The mirror video RTP pipeline additionally caps jitter, NAL aggregation, FU-A
assembly, marker-delimited access units, and sequence recovery. It rejects
malformed STAP-A/FU-A boundaries, timestamp changes before an access-unit
marker, unsupported payload types, and partial-frame loss without fabricating
media. It does not authenticate RTP, conceal loss, decrypt, decode, or own UDP
sockets. The separate RTP socket adapter allocates one bounded datagram buffer
and returns malformed-packet or OS errors without retaining input; callers must
still enforce the RTSP-negotiated peer, SSRC, payload type, and encryption
policy because the adapter is not an authentication boundary.

The mirror-audio RTP pipeline validates the negotiated payload type, PCM sample
alignment, packet capacity, and explicit sequence-gap recovery. It does not
authenticate or decrypt RTP, conceal loss, own sockets, or authorize a sender;
callers must keep it behind the authenticated AirPlay boundary. The
`MirrorMediaCoordinator` adds peer-IP and per-track SSRC filtering plus reset,
but those values are only trustworthy when derived from an authenticated RTSP
session; the coordinator is not an Apple pairing or encryption boundary.

The mirror RTSP session validates CSeq, session transitions, UDP ports,
dimensions, orientation, audio sample rate, and bounded XML/binary plist
configuration before entering Streaming or applying a streaming SETUP
reconfiguration. Reconfiguration is committed only after the complete new
configuration validates, so malformed updates cannot partially replace the
old one. The experimental `MirrorRtspTcpServer` adds a one-request TCP
connection boundary with read deadlines, request-byte bounds, and request-batch
limits, but it remains unauthenticated and does not provide TLS or Apple
pairing. Callers must authenticate the sender and coordinate socket/pipeline
reset and connection deadlines before exposing it to an untrusted network.

The mirror audio contract caps access-unit bytes and duration, restricts sample
rates/channels, and requires PCM16 payload alignment. It does not decrypt,
decode, conceal loss, or provide an audio output authorization boundary.

The experimental `RaopAudioCoordinator` rejects RAOP packets whose source IP,
SSRC, or negotiated payload type does not match the caller-provided session
binding before they enter the jitter buffer. These values are only trustworthy
when obtained from an authenticated RTSP boundary; the coordinator itself does
not implement Apple pairing, FairPlay/AES-CTR decryption, or sender
authorization.

The `frameark-diagnostics` recorder keeps only bounded, caller-redacted
summaries in memory. It strips control characters and omits session identifiers
from transition records, but it cannot know whether a caller supplied a secret
in a message. Integrations must avoid credentials, raw URLs, packet payloads,
device identifiers, and user media, and must obtain consent before exporting a
snapshot or persisting it outside the process.

The Android Receiver now gates its foreground lifecycle on a successful
Keystore-backed identity load. Private key material is not exported to Kotlin,
Rust, logs, notifications, or diagnostics; only a validated public-key
fingerprint is retained in the identity state. This is an identity-availability
gate, not sender authorization or a substitute for Rust trust approval.

The Android JNI trust calls delegate peer add/refresh, explicit revoke, and
fingerprint matching to the bounded Rust `TrustRegistry`. A changed fingerprint
is rejected until revocation; capacity and labels remain bounded. Rust itself
rejects add/refresh, raw-key enrollment and revoke while running or stopping,
under the same lock used by startup. A lifecycle remains active until the old
listener is joined, preventing restart or trust mutation against a stale
startup snapshot. Callers must stop successfully before revoking; a rejected
revoke never means the running connection has been deauthorized. Live session
revocation is not implemented. The snapshot
JNI calls expose only bounded public `FTR1` bytes and reject replacement during
an active session. `AndroidTrustSnapshotStore` encrypts those bytes with an
Android Keystore AES-GCM key and atomically replaces an app-private file. The
experimental settings surface blocks mutations while streaming and requires
the operator to enter the displayed identity values, but typed values are not
proof of sender authentication or key possession; pairing-driven approval must
replace this surface before a Stable claim.

The JNI lifecycle requires that public identity before opening its network
listener. Rust validates the bounded ASCII alias and SHA-256 fingerprint, uses
a fingerprint-derived device ID (not the shared Keystore alias), and rejects
identity changes during an active session. Reinstalling the same identity is
idempotent and stop/recovery retains it in process memory. Neither the identity
nor fingerprint is logged. No private key crosses JNI. This does not bind the
Keystore key to the ephemeral TLS certificate, prove key possession, or create
remembered peer authorization; the existing pinned TLS/PIN boundary is unchanged.

## Disclosure and fixes

Maintainers will validate the report, determine affected versions, coordinate a fix and regression test, and agree on disclosure timing with the reporter when practical. Security releases should include an advisory, upgrade guidance, affected-version range, checksums, and credit unless anonymity is requested.

Sanitized regression fixtures may be retained after disclosure. Raw secrets, personal media, and identifying packet captures must not enter the repository.
