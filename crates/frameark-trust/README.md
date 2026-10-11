# frameark-trust

`frameark-trust` is the bounded, protocol-independent trust policy shared by
FrameArk pairing adapters.

 It validates lowercase colon-separated SHA-256 public-key fingerprints and
 can derive that canonical fingerprint from a raw 32-byte public key,
 associates them with bounded `DeviceId` records, rejects silent key changes,
 supports explicit revocation, and redacts fingerprints from `Debug` output.
 It also provides a bounded deterministic `FTR1` snapshot codec for a
 platform-owned secure storage adapter. PendingTrustRequests provides an
 ephemeral, capacity-limited, monotonic-expiry approval queue that removes a
 request only after explicit approval, rejection, or expiry.
The registry is intentionally in-memory: a platform adapter must persist
records through its secure storage and must perform the actual signature or
 certificate verification before enqueueing or approving a request.

It does not generate keys, verify signatures, persist pending requests, or
authenticate a sender. The `trust_public_key` helper is only a canonical
fingerprint bridge for callers that have already verified key possession and
obtained explicit user approval. Its compatibility status is **Experimental**. The
snapshot contains no private key material and is not itself encrypted or
authenticated.
