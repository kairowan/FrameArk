# frameark-cli

The CLI currently provides one deliberately narrow M1 lab command:

```text
frameark-cli send --host 127.0.0.1 --port 4433 --server-name localhost \
  --certificate server.der --pairing-code 123456 \
  --video-fixture sample.h264 --audio-fixture sample.opus --frames 30
```

It performs pinned temporary FANP pairing, opens one bounded media stream, and
sends the bytes from the supplied encoded access-unit fixture files as FAM1
video/audio packets. Fixture files are capped before reading and the command
does not decode, transcode, or invent codec data. The first packet is marked a
video keyframe; callers must provide fixtures appropriate for the negotiated
codec. The command is currently for the Rust/native loopback and protocol-lab
workflow. Full Offer/Start/Stop control orchestration, mDNS discovery, and a
user-facing sender are still roadmap work.
