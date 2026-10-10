# ADR 0030: Bounded AirPlay/RAOP RTP jitter buffer

- Status: Accepted
- Date: 2026-10-11
- Scope: M4 AirPlay/RAOP media timing foundation

## Decision

Add `RtpJitterBuffer` to `frameark-airplay` for unencrypted RAOP RTP packets.
The buffer accepts at most 128 packets (or a smaller caller-selected capacity),
orders packets by 16-bit sequence number including wrap-around, rejects
duplicates, counts and drops packets arriving behind the consumed window, and
returns explicit backpressure when full. Consumers can call
`skip_missing_to` after their own deadline to advance over a loss gap.

The buffer owns no socket, clock, decoder, or retransmission policy. It exposes
only deterministic packet ordering and bounded loss-window state so a future
RAOP transport can add authenticated/encrypted payload handling and a platform
audio clock without changing parser ownership.

## Evidence and limits

Unit tests cover out-of-order packets, gap recovery, capacity limits,
duplicates, late packets, and sequence wrap. Apple pairing, FairPlay/AES-CTR,
packet authentication, concealment, decoder integration, and long-running
device timing remain unimplemented and Experimental.
