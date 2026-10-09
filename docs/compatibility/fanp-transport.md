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
| Malformed/oversized frame bounds | Parser rejects before payload interpretation | `frameark-transport` bounded frame implementation |

## Explicit limits

- No persistent trust or device identity is established.
- No media, audio, input, clipboard, file-transfer, or management streams are
  implemented.
- No internet relay, NAT traversal, Cast certification, Miracast guarantee, or
  DRM/HDCP behavior is claimed.
- The self-signed certificate is generated per server process and must be
  delivered and pinned by an in-scope caller.

Before this entry can become Beta, add Android and desktop loopback tests,
network interruption/retry coverage, and a named receiver/sender matrix.
