# frameark-airplay

`frameark-airplay` is the bounded Rust protocol layer for experimental
AirPlay/RAOP interoperability.

## Current capability

- bounded RTSP/1.0 request/response parsing with CSeq checks;
- strict RTP v2 audio packet contracts without padding/extensions;
- bounded ANNOUNCE SDP parsing for PCM, Apple Lossless, and AAC labels;
- bounded XML Property List parsing/encoding for strings, integers, booleans,
  data, arrays, and dictionaries;
- bounded H.264 Annex-B/AVCC mirror access-unit parsing with keyframe and
  orientation metadata plus a 90 kHz A/V clock policy;
- an explicit OPTIONS → ANNOUNCE → SETUP → RECORD → FLUSH/TEARDOWN RTSP
  session state machine with caller-owned UDP ports.

The session does not own TCP/UDP sockets, decode audio, or perform Apple
pairing. It is a deterministic protocol contract that a future daemon can
connect to platform audio and timing adapters.

FairPlay, AES-CTR decryption, binary plist negotiation, pairing, AirPlay 2,
H.265, mirror audio transport, and Apple device compatibility are not
implemented or implied. The
compatibility label is **Experimental**.

```powershell
cargo test -p frameark-airplay --all-features --locked
```
