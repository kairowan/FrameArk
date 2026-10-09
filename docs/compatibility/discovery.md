# Discovery compatibility

The M0 discovery prototype publishes and browses the FrameArk Native Protocol
DNS-SD service type `_frameark._udp.local.`. It is currently **Experimental**:
the service contract and event mapping are stable for this milestone, but there
is not yet a complete pairing or transport implementation.

Supported in this slice:

- IPv4 and IPv6 addresses reported by `mdns-sd`.
- Automatic address refresh across active interfaces.
- Optional interface-name allowlist for publication.
- Bounded UTF-8 instance, host, and TXT metadata.
- Service found, resolved, removed, and browse lifecycle events.

Not supported yet:

- Pairing, trust decisions, or transport authentication.
- Android multicast lifecycle integration.
- A promise of discovery across AP-isolated or internet-only networks.

For a host with multicast enabled, run the opt-in network smoke test after the
transport layer lands. Deterministic unit tests intentionally do not require a
multicast-capable CI runner.
