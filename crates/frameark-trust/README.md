# frameark-trust

`frameark-trust` is the bounded, protocol-independent trust policy shared by
FrameArk pairing adapters.

It validates lowercase colon-separated SHA-256 public-key fingerprints,
associates them with bounded `DeviceId` records, rejects silent key changes,
supports explicit revocation, and redacts fingerprints from `Debug` output.
It also provides a bounded deterministic `FTR1` snapshot codec for a
platform-owned secure storage adapter.
The registry is intentionally in-memory: a platform adapter must persist
records through its secure storage and must perform the actual signature or
certificate verification before calling `trust`.

It does not generate keys, implement cryptography, persist secrets, or decide
whether a user approved a pairing request. Its compatibility status is
**Experimental**. The snapshot contains no private key material and is not
itself encrypted or authenticated.
