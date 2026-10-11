# ADR 0038: One-shot DLNA SSDP multicast notify

- Status: Accepted
- Date: 2026-10-11
- Scope: M3 DLNA/UPnP discovery boundary

## Decision

Add `SsdpPublisher::notify_multicast` for one bounded `ssdp:alive` or
`ssdp:byebye` datagram to `239.255.255.250:1900`. The helper sets a low IPv4
multicast TTL and reuses the existing deterministic announcement encoder.

It is deliberately one-shot: a daemon owns interface selection, startup
bursts, cache-age scheduling, shutdown sequencing, and host-specific multicast
permissions. No multicast listener or peer authorization is added here.

## Evidence and limits

The publisher test exercises both alive and byebye sends from an ephemeral
loopback-bound socket and existing tests continue to validate the exact wire
format. Host multicast delivery, interface matrices, periodic scheduling, and
named DLNA client interoperability remain unverified and Experimental.
