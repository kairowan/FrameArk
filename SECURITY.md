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

Capability offers are exchanged only after the pinned TLS handshake. The
current schema requires an exact version match and rejects unknown or duplicate
entries before allocation; it does not silently downgrade or authorize media,
remote input, file transfer, or internet relay.

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
it is not a file/network fetcher. GENA, persistent authorization, and a
streaming media server are not enabled. The synchronous TCP adapter adds a
read timeout, complete-request bound, response-size bound, and closes each
connection after one response; it does not provide persistent authorization or
multi-client scheduling. The GENA registry caps subscriptions, leases,
callback schemes, XML property names/values, and pending events but does not
own lease expiry or callback sockets; a daemon must add authorization policy,
expiry, and event-queue limits at its socket boundary.
`GenaEvent::encode_http_notify` validates the callback authority and target and
caps the generated request. `GenaCallbackClient` adds bounded plain-HTTP
connect/read/write timeouts and exact-length response parsing, rejects HTTPS
unless a caller-owned TLS adapter is supplied, and does not provide callback
authentication, retries, lease expiry, or SSRF policy beyond URL/authority
validation.

The experimental RAOP session rejects unknown codecs, payload mismatches,
interleaved TCP transport, invalid state transitions, and oversized SDP before
allocating media state. It does not authenticate Apple senders or decrypt
protected audio; callers must not expose the unauthenticated state machine to
untrusted networks or treat it as a FairPlay boundary. XML and binary plist
parsers enforce document, object-table, reference, depth, and scalar-size
bounds; binary plist cycles and unsupported object types are rejected before
allocation of nested values.

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

The AirPlay XML Property List boundary caps document size, nesting, entries,
scalar/data fields, duplicate keys, and base64 decoding. It is not a parser for
binary plists or a FairPlay key container; callers must keep pairing secrets out
of diagnostics and only pass authenticated metadata into future crypto code.

The mirror video contract caps access units and NAL counts, rejects forbidden
H.264 headers, malformed length prefixes, unsupported orientation values, and
unbounded A/V offsets before platform allocation. It does not authenticate,
decrypt, decode, or render incoming video; those network and platform layers
must remain behind the receiver's authorization boundary.

The mirror RTSP session validates CSeq, session transitions, UDP ports,
dimensions, orientation, audio sample rate, and bounded XML/binary plist
configuration before entering Streaming. It owns no socket or authorization;
callers must authenticate the sender and enforce connection deadlines before
exposing it to an untrusted network.

The mirror audio contract caps access-unit bytes and duration, restricts sample
rates/channels, and requires PCM16 payload alignment. It does not decrypt,
decode, conceal loss, or provide an audio output authorization boundary.

## Disclosure and fixes

Maintainers will validate the report, determine affected versions, coordinate a fix and regression test, and agree on disclosure timing with the reporter when practical. Security releases should include an advisory, upgrade guidance, affected-version range, checksums, and credit unless anonymity is requested.

Sanitized regression fixtures may be retained after disclosure. Raw secrets, personal media, and identifying packet captures must not enter the repository.
