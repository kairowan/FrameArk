# ADR 0132: Complete v1 service declarations for the single-resource renderer

- Status: Accepted, Experimental
- Date: 2026-10-15

## Outcome

Replace action-name-only SCPD with ordered input/output arguments, related state
variables, types, ranges, allowed values and correct event flags for AVTransport:1,
RenderingControl:1 and ConnectionManager:1. The single-resource profile exposes
all required v1 actions plus Pause and master volume: 12, 4 and 3 actions respectively.
No certification, named-client compatibility or full M3 completion is implied.

The declarations are independently implemented from the public specifications
below, not copied service-template XML. One static contract drives both SCPD and
required-input validation. Rust still owns all protocol state and decisions.

## Behavior and migration

- Queries return every standard output in order, including metadata, track 0/1,
  network/empty media, unsupported recording/next-resource fields and i4 maximum
  counter sentinels. Absolute time equals relative time for this one-track profile.
- GetDeviceCapabilities reports NETWORK and no recording; GetTransportSettings
  reports NORMAL. Recording, repeat/shuffle, playlists, next-resource queuing, mute
  and other optional controls are not advertised.
- TRACK_NR seek accepts track 1 and dispatches a real zero-position seek. Next and
  Previous do not wrap a single track; they return 711 (701 if no source exists).
- ListPresets exposes FactoryDefaults. SelectPreset restores master volume 50 only
  after output acknowledgment, including when selected before loading media. No
  image-processing defaults or system-wide device reset is implied.
- Replacing a playing source prepares then resumes the new output; paused remains
  paused, and an initial source becomes stopped. If preparation succeeds but resume
  fails, output is invalidated and shut down rather than pretending the old source
  survived. On Android each command has a five-second limit; a replacement may
  require two commands plus the existing bounded source probe, not five seconds
  for the whole composite operation.
- Connection 0 is a static input connection, not Output. Unknown negotiated
  protocol/status information is empty/Unknown; there is no fabricated codec proof.
  Nonzero connection IDs return 706, malformed signed IDs 402. Invalid AVTransport
  instance returns 718; RenderingControl returns 702. Unsupported speed returns 717.
- Fatal output failure is retained as ERROR_OCCURRED in queries and LastChange,
  even if a later command fails because the backend has been detached. The public
  pre-alpha Rust state struct adds transport_error; struct-literal callers must
  initialize it or use Default. No C/JNI ABI or persistence schema changes.

All mandatory inputs are now required, even InstanceID, Speed and an empty
CurrentURIMetaData element. Missing, extra, duplicate or malformed inputs fail
before output dispatch. Callers that relied on implicit values must send complete
requests. XML line endings are normalized by the parser; escaped metadata content
is retained as text, not parsed or fetched as an external document. LastChange
includes supported non-position state, including preset names and error status.

## Parsing, dependency and limits

Use roxmltree 0.21.1 (MIT OR Apache-2.0; MIT selected by the existing notice policy)
for namespace-aware SOAP structure instead of substring extraction. It adds one
locked package, no extra transitive packages. Cargo archive verification and notice
generation include its upstream license texts. The parser has no entity resolver;
DTDs are rejected, including empty DTDs. Input remains <=16 KiB, <=128 XML nodes,
flat scalar arguments and existing per-argument/4 KiB decoded-metadata bounds.
Only one correctly namespaced Envelope/Body/action is accepted; body action must
match SOAPAction and the service route. Nonempty SOAP headers, nested arguments,
foreign argument namespaces and unsupported arguments are rejected. This is a
bounded, deliberately narrow UPnP SOAP profile, not a general SOAP processor.

Control input validation is not sender authentication or media-fetch authorization.
Existing private-IPv4 consent, peer-pinned origin, no redirects, explicit TLS roots,
redaction, output acknowledgment, subscription limits and callback policy remain.
Unknown XML fields fail safely; no unchecked field can become an output command.

## Verification and remaining work

Rust tests independently assert the v1 inventory and output order, resolve every
SCPD argument reference, check types/event flags, exercise strict parser limits,
service-specific faults, output rejection/fatal cleanup, track/preset acknowledgment
and replacement state. An actual TCP controller fetches each SCPD, constructs input
arguments from it and invokes all 19 advertised actions. Decoder tests additionally
exercise FactoryDefaults, TRACK_NR and automatic playing-source replacement through
Android MediaPlayer. These fixtures are not a third-party controller matrix.
See the [verification record](../evidence/2026-10-15-dlna-service-contracts/README.md).

M3 still needs images/subtitles, wider authorized origins and Android HTTPS,
MP3/codec coverage, SSDP MX scheduling/interface/port interoperability, named
Android/iOS/desktop control clients and two-hour playback/Seek. Keep Experimental.
Rollback is a reviewed revert; no keys, tags, releases or data migration are involved.

## Primary sources

- [UPnP AVTransport:1, sections 2.2–2.4](https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf)
- [UPnP RenderingControl:1, presets and volume](https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf)
- [UPnP ConnectionManager:1, static connection 0](https://upnp.org/specs/av/UPnP-av-ConnectionManager-v1-Service.pdf)
- [roxmltree 0.21.1 parsing options](https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html)
- [roxmltree upstream MIT license](https://docs.rs/crate/roxmltree/0.21.1/source/LICENSE-MIT)
