# FrameArk architecture decisions

Read this reference when deciding module placement, protocol support level, FFI ownership, or product scope. `PLAN.md` remains the full source of truth.

## Product pillars

1. A reusable Rust receiver, media, session, discovery, trust, and diagnostics core.
2. Stable AirPlay/RAOP and DLNA interoperability built from independently reviewable protocol layers.
3. An open, versioned FrameArk Native Protocol (FANP) controlled by the project.

FrameArk is a local-first ecosystem rather than a single Android APK. Android, desktop, browser, daemon, SDK, CLI, Web Admin, and protocol-lab products share the same core contracts.

## Dependency direction

```text
Apps and UI
    ↓
Platform adapters and FFI
    ↓
Media session, clock, buffers, and codec model
    ↓
Protocol adapters
    ↓
Transport and discovery
    ↓
Identity, trust, configuration, events, and metrics
```

Dependencies flow downward. Protocol crates do not own Android `Surface`, MediaCodec, VideoToolbox, VAAPI, UI, or platform lifecycle objects.

## Control and media planes

- The control plane owns discovery, authorization, negotiation, playback commands, state, and feedback.
- The media plane owns video, audio, subtitles, metadata, and file payloads.
- Ordinary FFI messages are suitable for control events. High-frequency media uses direct/shared buffers, opaque handles, or platform surfaces.
- Every track has an explicit time base, sequence space, codec configuration, backpressure policy, and lifecycle.

Map every protocol into the unified session sequence:

```text
Idle → Connecting → Authenticating → Negotiating → Preparing
     → Streaming ↔ Reconfiguring → Draining → Closed
                         ↓
                     Recovering
```

## Protocol maturity

| Area | Product treatment |
|---|---|
| FANP over QUIC/TLS 1.3 | Primary protocol; versioned public specification and conformance fixtures |
| DLNA/UPnP | Stable target based on public specifications and tested clients |
| AirPlay/RAOP | Compatibility claims only for tested device/OS combinations |
| Cast V2 | Compatibility mode; no Google certification claims |
| Miracast/WFD | Conditional platform adapter requiring sink capability and system support |
| FairPlay, Widevine, HDCP bypass | Unsupported |

Use Stable, Beta, Experimental, Conditional, and Unsupported consistently in code, UI, documentation, and release notes.

## FANP invariants

- Discover with mDNS/DNS-SD using `_frameark._udp` unless an accepted ADR changes it.
- Use QUIC and TLS 1.3; do not invent cryptographic algorithms.
- Use reliable streams for control, configuration, and files; use datagrams where low-latency media benefits.
- Store long-term private keys in platform security storage.
- Version messages and capabilities so unknown optional fields can be ignored safely.
- Bound message sizes, nesting, queues, retransmission, and memory.
- Include redacted trace points and deterministic fixtures for negotiation and recovery.

## Decision records

Create an ADR when a change affects protocol wire format, crate boundaries, public API/ABI, persistence, trust, security posture, compatibility promises, or platform ownership. Link the ADR from the implementing pull request and update `PLAN.md` if the product roadmap changes.
