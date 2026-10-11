# ADR 0033: Bounded AirPlay mirror audio contract

- Status: Accepted
- Date: 2026-10-11
- Scope: M5 AirPlay screen-mirroring media boundary

## Decision

Add `MirrorAudioFormat` and `MirrorAudioAccessUnit` to
`frameark-airplay`. The contract accepts AAC or interleaved PCM16 labels,
sample rates from 8 kHz through 192 kHz, one through eight channels, bounded
payloads up to 256 KiB, and access-unit durations up to one second at the
maximum supported sample rate. PCM16 payloads must align to complete channel
samples. Timestamps stay in the audio sample clock and can be converted by the
shared `MirrorClock`.

This is a media ownership contract only. It does not specify AirPlay pairing or
encryption, RTP transport, packet loss concealment, AAC/PCM decoding, or
Android/Desktop audio output. Platform adapters remain responsible for those
steps after the sender is authorized.

## Evidence and limits

Unit tests cover AAC and PCM16 creation, timestamp end calculations, sample
rate/channel bounds, duration limits, PCM alignment, and overflow rejection.
Real mirror audio captures, decoder output, synchronization drift, and Apple
device compatibility remain unverified and Experimental.
