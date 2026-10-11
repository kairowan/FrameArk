# frameark-cli

The CLI provides bounded M1 lab and management commands:

```text
frameark-cli discover --timeout-ms 3000 --max-events 64
frameark-cli connect --host 127.0.0.1 --port 4433 --server-name localhost \
  --certificate server.der --pairing-code 123456
frameark-cli status --host 127.0.0.1 --port 4433 --server-name localhost \
  --certificate server.der --pairing-code 123456
frameark-cli end --host 127.0.0.1 --port 4433 --server-name localhost \
  --certificate server.der --pairing-code 123456
```

`discover` browses the reserved `_frameark._udp.local.` service for a bounded
time and prints normalized events. `connect` performs temporary pinned pairing,
`status` issues one bounded FANP status request, and `end` sends a bounded Stop
request. Each management command uses a fresh temporary session and never
persists trust or credentials.

The media lab command remains deliberately explicit:

```text
frameark-cli send --host 127.0.0.1 --port 4433 --server-name localhost \
  --certificate server.der --pairing-code 123456 \
  --video-fixture sample.h264 --audio-fixture sample.opus --frames 30
```

It performs pinned temporary FANP pairing, negotiates one Offer, starts the
session, opens one bounded media stream, sends the bytes from the supplied
encoded access-unit fixture files as FAM1 video/audio packets, and finishes
with Stop. Fixture files are capped before reading and the command does not
decode, transcode, or invent codec data. The first packet is marked a video
keyframe; callers must provide fixtures appropriate for the negotiated codec.
The command is currently for the Rust/native loopback and protocol-lab
workflow. Reconnect, dynamic reconfiguration, persistent trust, and a
user-facing sender are still roadmap work.
