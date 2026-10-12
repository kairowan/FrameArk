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
- bounded H.264 Annex-B/AVCC and H.265 Annex-B mirror access-unit parsing with
  codec, keyframe, and orientation metadata plus a 90 kHz A/V clock policy;
- bounded mirror video RTP pipeline for H.264 single-NAL/STAP-A/FU-A and H.265
  single-NAL/AP/FU payloads, marker-delimited access units, sequence recovery,
  and orientation metadata;
- bounded caller-owned UDP RTP socket parsing that returns each packet's source
  address for higher-level RTSP policy enforcement;
- bounded mirror RTSP session state with OPTIONS/SETUP/RECORD,
  GET_PARAMETER/FLUSH/TEARDOWN transitions, UDP transport validation, XML/
  binary plist video configuration checks (explicit H.264/H.265 codec gate),
  and atomic SETUP reconfiguration
  while streaming;
- bounded mirror AAC/PCM16 audio format and access-unit contracts with sample
  timestamps, duration, channel, and payload-size validation;
- an explicit OPTIONS → ANNOUNCE → SETUP → RECORD → FLUSH/TEARDOWN RTSP
  session state machine with caller-owned UDP ports.
- bounded RAOP `SET_PARAMETER`/`GET_PARAMETER` text controls for volume,
  sample progress, and a small title/artist/album/genre/composer metadata set.
- bounded ordered RAOP PCM16 conversion from network-byte-order RTP samples to
  little-endian platform buffers, with negotiated payload/codec/channel checks;
  ALAC/AAC remain decoder-owned and unsupported here.
- bounded HLS media-playlist parsing with target/sequence/end-list state,
  segment duration limits, and relative/HTTP(S) URI traversal/credential
  checks;
- a caller-owned HTTP/HTTPS HLS playlist and segment connector boundary with
  bounded response headers/body, content-length checks, MIME validation, and
  explicit no-redirect behavior;
- an optional bounded FIFO HLS segment cache with URI validation, duplicate
  replacement, entry/byte budgets, and oldest-entry eviction; DNS, TLS,
  redirects, authentication, and decoding remain caller-owned.
- caller-owned peer authorization hooks on the RAOP and mirror RTSP TCP
  adapters, evaluated before request bytes enter the session state machine.
- a redaction-safe mirror compatibility report that explicitly classifies
  clear-IP, AWDL, FairPlay/DRM, unknown transport, and unverified-pairing
  inputs instead of silently attempting an unsupported path.

The RTSP session does not own a TCP socket, and the RTP socket adapter does not
join multicast groups or enforce the negotiated peer, SSRC, payload type, or
encryption policy. These remain caller-owned decisions. The crate does not
decode audio or perform Apple pairing; it is a deterministic protocol contract
that a future daemon can connect to platform audio and timing adapters.

FairPlay, AES-CTR decryption, pairing, DMAP metadata, AirPlay 2, platform
decoding, and Apple device compatibility are not implemented or implied. The
compatibility report is an admission/diagnostics boundary only; it is not an
Apple pairing implementation and it never authorizes protected content.
H.265
RTP assembly is a bounded conditional protocol capability only: an explicit
H.265 SETUP is accepted, but callers must still provide authenticated RTSP
policy and a platform decoder. The
compatibility label is **Experimental**.

```powershell
cargo test -p frameark-airplay --all-features --locked
```
