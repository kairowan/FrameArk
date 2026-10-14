# ADR 0120: installable foreground Native reference sender

- Status: Accepted for Experimental use
- Scope: M1 reference sender, not M2 screen capture or a milestone exit

Later additive extensions: [ADR 0121](0121-android-screen-capture.md) adds screen
capture; [ADR 0124](0124-android-sender-discovery.md) adds untrusted discovery
candidates while retaining the explicit fingerprint/PIN gate described here.

## Decision

Ship `dev.frameark.sender` as a separate ARM64 APK alongside Receiver. Android
MediaCodec encodes a moving EGL test pattern (720p30 or 1080p30) and stereo
48 kHz AAC-LC tone pulses for 30 seconds. No screen or microphone is captured.
Both apps use the same `frameark-ffi` library. Rust retains TLS/PIN, Native
Offer/Start/media/Stop, sequence spaces and explicit microsecond timebases.

A sender generation owns one continuously driven Tokio worker. Commands are
bounded to 128 entries and 8 MiB queued encoded payload; each video is at most
4 MiB, each audio at most 64 KiB. Kotlin copies codec buffers before releasing
them, JNI makes one bounded owned copy, and Rust frames it. This reference path
is **not zero-copy**; allocation/throughput optimization requires measurements.
Submission cannot block the UI. A full queue gives explicit backpressure with a
one-second producer budget, never silent frame loss. Setup/media operations
have five-second budgets, idle streaming ten seconds, total worker sixty
seconds. Cancel polls every fifty milliseconds, also during connect and send;
driver calls remain OS-owned and cannot be given hard realtime guarantees.
Finish is ordered after queued frames and disallows later submission. Its
300 ms output grace is not an audible-output acknowledgment. Stale generation
handles cannot submit, stop or cancel a subsequent run.

## Out-of-band local pairing ticket

Rust `PairingTicket` encodes exactly three ASCII lines, no final newline:

```text
frameark-pair-v1
<RFC1918-or-link-local-IPv4>:<nonzero-port>
<lowercase-hex-public-DER-certificate>
```

Certificate size is 1..4096 bytes, total text at most 8256 bytes. Parser rejects
unknown versions/fields, DNS/URLs, public/loopback/multicast/unspecified IPv4,
IPv6, noncanonical endpoints, mixed/invalid hex and oversized input. TLS itself
validates the certificate using the fixed `frameark.local` name and exact pin.
This is a bootstrap UX format, not a new transport message or mDNS replacement.
The six-digit PIN is **not** in the ticket. No caller-supplied TLS name or
insecure fallback is available. Tickets expire when Receiver restarts.

Receiver enumerates OS interface addresses; Rust admits only local candidates.
The operator chooses an address and explicitly copies/shares through Android.
It shows the full certificate SHA-256. Sender never auto-reads clipboard or
intents; the user pastes, inspects, confirms the full matching fingerprint and
enters PIN separately before Send. Confirmation binds the exact inspected
ticket. No ticket/PIN is persisted by Sender; sensitive fields disable state
saving/autofill, screenshots are blocked, errors retain only static source
location and numeric stage. The OS clipboard/share recipient remains outside
FrameArk's control. Use a trusted channel; fingerprint comparison is essential
because possession of an attacker-supplied ticket is not authenticated trust.

This is ephemeral certificate trust, **not** the Receiver Keystore identity,
proof-capable Sender, remembered peer enrollment, QR pairing, or discovery UI.
Proof-required Receiver mode correctly rejects this sender. Optional one-shot
post-PIN approval still requires Receiver's explicit arm action.

## Lifecycle and compatibility

Foreground only. Sender cancels on pause/rotation; no automatic continuation.
Receiver may retain an **idle** listener while sharing; surface detach and
active-output detection are synchronized. Active surface loss releases output
and stops reception. A new configuration without a surface fails closed, so
return to Receiver before Send. Both apps keep their foreground screen on.

Video is H.264 baseline Annex B with SPS/PPS repeated at IDR. Incompatible
encoders fail explicitly; there is no silent resolution/codec downgrade. AAC
access units are raw AAC-LC, not ADTS. Initial negative encoder timestamps are
discarded. The synthetic one-second flash/tone supports visual observation,
not a calibrated latency/lip-sync measurement. No Opus sender, rotation,
recovery, background capture, physical hardware or Apple support is implied.

## Verification and sources

Ticket negative/boundary tests, bounded JNI queue tests, wrong-PIN/cancelled
real-QUIC setup, JVM confirmation/Annex-B tests and two-app instrumentation
cover this increment. The two-emulator fixture exports temporary credentials
only into app-private test files and deletes them after the run. It does not
add exported production components or permission bypasses. CI compiles both
instrumentation APKs and verifies native packaging in both app APKs; emulator
runtime and physical operator tests are separate evidence.

Platform implementation follows Android's [MediaCodec contract](https://developer.android.com/reference/android/media/MediaCodec)
and [EGL presentation timestamps](https://developer.android.com/reference/android/opengl/EGLExt).
No new third-party dependency or incompatible source code was added.

Rollback: remove Sender app/ticket UI; existing FANP transport and Receiver
compatibility are unchanged. Future increments add discovery, real screen
capture, proof-capable identity, measured clocks and user hardware evidence.
