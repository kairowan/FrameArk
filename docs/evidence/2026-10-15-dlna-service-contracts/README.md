# Single-track DLNA v1 contract regression — 2026-10-15

This record covers ADR 0132, not certification, physical-device interoperability
or the M3 two-hour gate. No owner device was connected or controlled.

## Local verification

- Rust 1.97: 282 workspace tests passed, including 95 DLNA tests; the existing
  operator-authorized physical multicast LAN test remains ignored.
- Ten new Rust tests cover complete action inventory, argument/state references,
  ordered query outputs, exact error codes, parser shape/size/node limits, byte
  mutation smoke, complete initial LastChange, output-acknowledged presets/track
  seek, and playing/paused replacement/failure cleanup. One controller obtains
  all three SCPDs and invokes all 19 advertised actions over actual loopback TCP.
- Workspace fmt, all-target/all-feature Clippy and rustdoc with warnings denied pass.
- Receiver/Sender JVM reports: 96 + 11 passing cases; Receiver debug lint passes.
- All 23 Python policy/release/independent-XML tests pass.
- Normal ARM64 native library builds without the probe feature. SHA-256:
  `dc79314505c5071cab127bd65434ea07a7b0b5270ebffae978b6f259ab6f5db8`.
- Notice generation verifies 124 components and 257 original legal texts, including
  locked roxmltree 0.21.1. Both Android runtime inventories are checked.
- The original source-close timing regression passes 100 consecutive local
  Windows runs after the nonblocking correction. Two additional tests exercise
  synchronized cancellation without shutdown and unchanged deadline/terminal
  error handling; the one-second cancellation assertion is not relaxed.

## Isolated Android execution

Agent-owned API 34 x86_64 AVD `frameark_proof_ui`, serial `emulator-5582`, Windows
host, Android SDK/NDK 28.0.12433566, Java 21. The exact AVD was verified before
install/run and stopped afterward. Matching probe-only JNI SHA-256:
`07e6170acf03a51c5431b3e6cdc3ebcd5bb8c4368dc3f8ff22186e2fc57bca92`.
These x86_64 APKs are test artifacts, not owner-distribution builds.

Run `dev.frameark.receiver.DlnaPlaybackTest` with InstrumentationTestRunner on
that exact isolated serial. Three final runs after both the XML-depth and Windows
cancellation corrections passed all three cases (8.783, 8.486 and 8.084 seconds):

1. Rust byte adapter bounds, short reads, EOF and idempotent close.
2. Normal private-IPv4 foreground listener startup, page-close Rust joins and
   asynchronous OS-player release.
3. Original 160x120 H.264 MP4 decoded red TextureView pixels; actual progress;
   Play/Pause/REL_TIME Seek/Stop/replay; volume 27 then FactoryDefaults 50;
   TRACK_NR 1 returns to zero; Next/Previous fault at the single-track boundary;
   replacement while playing automatically starts the original 48 kHz mono PCM
   WAV, advances and reaches EOS without a second Play command.

Each final run reported 12 origin Range requests and one video-start notification.
Those counters are not decoded-frame counts or audible-quality evidence. Existing
pixel assertions still execute; UI layout was unchanged and no new UI screenshot
is claimed. Core service tests use an acknowledging fixture, while the Android
cases above exercise the real OS player.

## CI-driven correction

The first commit's local tests passed, but macOS CI aborted on the deeply nested
XML negative fixture: the DOM parser's node limit did not bound its recursive
stack use. The correction adds a nonrecursive four-element-depth check before
DOM parsing and a 128 KiB-thread regression for shallow valid and deeply nested
input. No stack-size increase, ignored test or CI bypass was used. The unapproved
initial preview was quarantined, not delivered.

The corrected XML revision passed macOS CI, but both Windows jobs exposed the
pre-existing active-source cancellation race: the read waited for its one-second
deadline despite close. The same assertion failed locally on repetition 29.
Connected I/O now uses nonblocking polling with cancellation/original-deadline
checks and at most one-millisecond waits; no test timeout or CI check was weakened.

During final Android rerun, a cold-boot instrumentation request remained registered
with target PID 0 before any test began. It was aborted by rebooting only the owned
AVD, waiting for completed boot, then rerunning; the three final passes above
followed. This startup attempt is not counted as a passing test.

After the cancellation fix, the first Android run passed byte-adapter/video/WAV
work but failed the separate foreground case because the cold-boot AVD had no
site-local IPv4 address (Wi-Fi was disabled). This is a failed run, not a pass.
Enabled networking only on the owned AVD and checked its 10.0.2.15 interface;
the three consecutive final full-suite passes above followed. No test was skipped
or changed to accept a missing network, and no owner's device was accessed.

## Remaining gates

Named Android/iOS/desktop controllers, physical devices, MP3/codec variants,
images/subtitles/playlists, broader origin approval/Android TLS, SSDP scheduling
and multi-interface/port interoperability, two-hour playback/Seek and M0–M5
physical performance/security acceptance remain open. Debug test signing is not
production signing; no production key, tag or release was created.
