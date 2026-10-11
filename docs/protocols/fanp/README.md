# FANP transport v1 (M0 experimental)

FrameArk Native Protocol (FANP) is the project-owned control protocol. This
document describes the first executable transport increment; it is not a
stable public wire-compatibility promise yet.

## Connection profile

- Transport: QUIC over UDP, provided by `quinn`.
- TLS: rustls TLS 1.3 with an ephemeral server certificate.
- ALPN: `frameark/1`.
- Scope: local network or test loopback only.
- Authorization: one six-digit decimal pairing code, valid only for the
  caller-owned process/session.
- Optional sender identity: Ed25519 challenge/proof, supplied by a caller-owned
  platform signer and verified in Rust before capability negotiation.

The client must receive the server certificate through an in-scope channel
and pin those exact DER bytes. mDNS metadata is discovery only and is not a
certificate authority.

After the code is accepted, a receiver may run either the caller-owned
address approval seam or the optional Ed25519 peer-proof flow before capability
negotiation. The address and pairing code alone are not sender identity proof.
The proof flow binds a fresh server challenge to a bounded device identifier
and public key; a caller-owned policy then decides whether the verified sender
may proceed. Verification is temporary and does not persist trust. A
production trust UI must still connect this proof to its Rust-owned registry
and platform keystore; typed fingerprints are not sufficient.

## Control frame

Every control stream starts with this 8-byte header:

| Offset | Size | Field | Encoding |
|---:|---:|---|---|
| 0 | 4 | Magic | ASCII `FANP` |
| 4 | 1 | Version | `1` |
| 5 | 1 | Message type | `1` request, `2` accepted, `3` rejected |
| 6 | 2 | Payload length | Unsigned big-endian |

Payloads are capped at 256 bytes. A peer that sends an invalid magic value,
unsupported version, or oversized payload is rejected without attempting to
interpret the remaining bytes.

### Pair request (`type = 1`)

The payload is exactly six ASCII decimal digits. The server compares it with
the process-local pairing code using the Rust transport state. No code is
written to logs or persistent storage.

### Pair accepted (`type = 2`)

The payload is empty. Receipt means that the pinned TLS connection is
authorized for the caller's temporary session. Media negotiation and track
streams are not part of M0.

### Pair rejected (`type = 3`)

The payload is empty. The client reports a pairing rejection separately from a
QUIC/TLS failure, then closes the connection.

### Optional peer proof (`types = 7, 8, 9`)

When the receiver uses the proof-enabled API, it opens a bounded challenge
stream with `type = 7` and a fresh 32-byte nonce. The sender opens a separate
proof stream with `type = 8` and sends:

| Offset | Size | Field | Encoding |
|---:|---:|---|---|
| 0 | 1 | Proof version | `1` |
| 1 | 1 | Device ID length | `1..=64` |
| 2 | N | Device ID | ASCII `[A-Za-z0-9_.-]` |
| 2 + N | 32 | Ed25519 public key | Raw bytes |
| 34 + N | 64 | Signature | Raw bytes |

The signature covers `"FrameArk FANP peer authentication v1\\0"`, the device
ID length and bytes, and the fresh challenge. The normal `ClientHello` then
uses a new stream and the receiver returns `ServerHello` as usual. Legacy
pairing uses the original `ClientHello` stream and remains compatible. A proof
failure is rejected before capability negotiation and does not create a
persistent trust record. A successful proof receives an empty `type = 9`
acknowledgement on the proof stream; a rejected proof receives the bounded
`NegotiationRejected` response and no `ClientHello` is accepted.

## Capability negotiation

After `PairAccepted`, the client opens a second reliable control stream and
sends `ClientHello` (`type = 4`). The server responds with `ServerHello`
(`type = 5`) containing the selected intersection. A peer with no common
capability receives `NegotiationRejected` (`type = 6`) and the session is
closed.

The ClientHello/ServerHello payload is bounded to 18 bytes:

| Offset | Size | Field | Encoding |
|---:|---:|---|---|
| 0 | 1 | Capability schema version | `1` |
| 1 | 1 | Entry count | `0..=16` |
| 2 | N | Capability IDs | One byte per entry, sorted and unique |

The canonical IDs are owned by `frameark-core`: `1` video, `2` audio, `3`
subtitles, and `4` remote control. Unknown IDs, duplicate entries, truncated
payloads, and schema versions other than `1` are rejected. The selected
intersection is exposed to the shared Rust session layer, which advances the
common lifecycle through `Connecting → Authenticating → Negotiating` without
letting a platform adapter create a parallel state machine.

## Compatibility and evolution

Post-handshake protocol adapters reserve message types 16..127. Each reliable
bidirectional stream carries exactly one request and one response followed by
FIN. Frames with trailing bytes are rejected. Each control operation has a
total deadline; cancellation, timeout, or a dropped response handle closes the
connection. Terminal responses wait for QUIC acknowledgement before teardown.
Pairing codes are redacted even when formatted with Rust `Debug`.

This profile is **Experimental**. Future versions must use a new ALPN or a
backward-compatible version negotiation rule and must preserve the maximum
frame bound. A stable release also needs persistent identity, replay handling,
capability negotiation, media stream definitions, and named sender/receiver
compatibility tests.

## Native control profile (Experimental)

The `frameark-native` crate maps control message types 16 and 17 to a bounded
request/response profile: `Offer`, `Start`, `Stop`, and `Status`. The current
offer supports optional H.264 video and Opus/AAC audio, a 10..=2000 ms latency
target, at most 3840×2160 at 60 fps, and audio up to 96 kHz/8 channels. A
receiver must advertise explicit policy limits; there is no silent codec,
resolution, or frame-rate fallback.

The flow is `Negotiating → Offer → Preparing → Start → Streaming → Stop →
Closed`. The response echoes a request ID and resulting shared state. A
platform backend is prepared before `Start`, and its reset hook is called on
stop, rejected preparation, timeout, cancellation, or receiver drop. The
profile is control-plane only: it does not carry encoded samples or claim that
an Android/desktop decoder is already wired.
