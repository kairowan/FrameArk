# framearkd

`framearkd` is an experimental local-network FANP reference receiver. It
accepts one or more temporary QUIC/TLS pairings, validates the Native control
sequence, drains bounded FAM1 media frames, and reports redacted frame and
resource counters.

It deliberately uses counting renderers. It does not decode H.264/Opus/AAC,
own a display or audio device, persist pairing trust, or claim a production
receiver/device compatibility result.

## Run a one-session fixture receiver

```text
cargo run -p framearkd -- --bind 127.0.0.1:4433 \
  --server-name localhost --pairing-code 123456 \
  --certificate-out target/framearkd.der --once
```

The daemon prints the selected address, pairing code, and certificate byte
count. Pass the generated DER path, address, server name, and code to
`frameark-cli send` with real encoded access-unit fixtures. The session ends
after the sender sends `Stop` or the bounded timeout expires.

Use `--once` to exit after one pairing; without it, rejected pairing attempts
are discarded and the listener continues accepting temporary sessions.
