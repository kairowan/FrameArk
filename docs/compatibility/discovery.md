# Discovery compatibility

The discovery adapter publishes and browses the FrameArk Native Protocol
DNS-SD service type `_frameark._udp.local.`. It is currently **Experimental**:
the service contract and event mapping are bounded, and the Android FANP
runtime now performs best-effort publication alongside its ephemeral QUIC
listener, but physical multicast and device coverage are not yet verified.

Supported in this slice:

- IPv4 and IPv6 addresses reported by `mdns-sd`.
- Automatic address refresh across active interfaces.
- Optional interface-name allowlist for publication.
- Bounded UTF-8 instance, host, and TXT metadata.
- Service found, resolved, removed, and browse lifecycle events.
- Android Rust/JNI publication with non-secret `proto`, `ver`, `tls`, and
  media capability TXT labels.
- Android foreground-service Wi-Fi multicast-lock acquisition and release are
  best effort and idempotent.

Not supported yet:

- Pairing, trust decisions, or transport authentication in DNS-SD itself.
- A guarantee that Android mDNS publication succeeds when the platform has no
  usable multicast interface or permission.
- A promise of discovery across AP-isolated or internet-only networks.

For a host with multicast enabled, run the opt-in network smoke test after the
transport layer lands. Deterministic unit tests intentionally do not require a
multicast-capable CI runner.
