# FANP transport compatibility

## Current status

**Experimental (M0, loopback/local-network test scope).** The Rust
`frameark-transport` crate has deterministic framing tests and an in-process
QUIC/TLS pairing test. No Android, desktop, or third-party sender has yet been
included in a compatibility matrix.

## Verified behavior

| Scenario | Result | Evidence |
|---|---|---|
| Loopback server with pinned certificate and correct six-digit code | Pass | `pinned_quic_pairing_completes_a_temporary_session` |
| Incorrect code | Pass; client and server report `PairingRejected` | `incorrect_code_is_rejected_without_persisting_trust` |
| Capability offer intersection | Pass; common capabilities are selected and exposed to core session mapping | `capability_intersection_advances_the_shared_core_session` |
| No common capability | Pass; both peers report a negotiation rejection | `no_common_capability_is_rejected` |
| Malformed/oversized frame or capability payload | Parser rejects before payload interpretation | `frame_header_rejects_bad_magic_versions_and_lengths`, `malformed_and_duplicate_entries_are_rejected` |
| Native Offer → Start → Status → Stop lifecycle | Pass on a fake backend; prepare/start/reset and core states are asserted | `frameark-native::native_control_reaches_prepare_start_status_and_stop` |

## Explicit limits

Control-stream tests additionally cover response delivery before close,
deadlines, dropped request cleanup, trailing-byte rejection, and redacted PIN
debug formatting. These run over real QUIC loopback, not a mocked transport.

- No persistent trust or device identity is established.
- No media, audio, input, clipboard, file-transfer, or management streams are
  implemented.
- Codec profiles, dimensions, frame rates, HDR, tracks, and datagram policy are
  intentionally bounded to the experimental native control profile; encoded
  samples and datagrams are still deferred.
- No internet relay, NAT traversal, Cast certification, Miracast guarantee, or
  DRM/HDCP behavior is claimed.
- The self-signed certificate is generated per server process and must be
  delivered and pinned by an in-scope caller.

Before this entry can become Beta, add Android and desktop loopback tests,
network interruption/retry coverage, and a named receiver/sender matrix.
