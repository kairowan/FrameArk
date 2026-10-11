# frameark-airplay

`frameark-airplay` is the bounded Rust protocol layer for experimental
AirPlay/RAOP interoperability.

## Current capability

- bounded RTSP/1.0 request/response parsing with CSeq checks;
- strict RTP v2 audio packet contracts without padding/extensions;
- bounded RTP jitter buffering with sequence reordering, wrap handling,
  duplicate/late rejection, and explicit loss-gap recovery;
- bounded ANNOUNCE SDP parsing for PCM, Apple Lossless, and AAC labels;
- bounded XML Property List parsing/encoding for strings, integers, booleans,
  data, arrays, and dictionaries;
- bounded binary Property List parsing for the same shared value subset,
  including cycle, reference, object-count, and nesting guards;
- bounded H.264 Annex-B/AVCC mirror access-unit parsing with keyframe and
  orientation metadata plus a 90 kHz A/V clock policy;
- bounded mirror video RTP pipeline for single-NAL, STAP-A, and FU-A payloads,
  marker-delimited access units, sequence recovery, and orientation metadata;
- bounded mirror RTSP session state with OPTIONS/SETUP/RECORD,
  GET_PARAMETER/FLUSH/TEARDOWN transitions, UDP transport validation, and
  XML/binary plist video configuration checks;
- bounded mirror AAC/PCM16 audio format and access-unit contracts with sample
  timestamps, duration, channel, and payload-size validation;
- an explicit OPTIONS → ANNOUNCE → SETUP → RECORD → FLUSH/TEARDOWN RTSP
  session state machine with caller-owned UDP ports.

The session does not own TCP/UDP sockets, decode audio, or perform Apple
pairing. It is a deterministic protocol contract that a future daemon can
connect to platform audio and timing adapters.

FairPlay, AES-CTR decryption, pairing, AirPlay 2,
H.265, mirror audio socket transport, and Apple device compatibility are not
implemented or implied. The
compatibility label is **Experimental**.

```powershell
cargo test -p frameark-airplay --all-features --locked
```
