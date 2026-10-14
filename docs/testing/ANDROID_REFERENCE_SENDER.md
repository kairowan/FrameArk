# Android reference Sender + Receiver preview

Experimental M1 test tool. Requires two Android 8+ ARM64 devices with a reachable
private IPv4 LAN. It sends generated picture and tone, **not your screen**.
For the separate Android 10+ real screen-sharing mode, use the
[capture guide](ANDROID_SCREEN_CAPTURE.md). This page covers synthetic mode only.
You operate your own devices; no developer tools, ADB access, private media,
packet captures or permanent device identifiers need to be shared with us.

## Operator steps

1. Install the matching Receiver APK on the display device and Sender APK on
   the source device. Preview APKs use the documented test signing identity,
   not a store/release signature. Do not uninstall existing data to work around
   a signature conflict without first deciding whether you need that data.
2. Connect both to the same trusted LAN. Disable guest/AP isolation if you
   control the router. VPN/private-interface addresses may be unreachable;
   choose the receiver's actual Wi-Fi address. Do not expose a router port.
3. Open Receiver, leave proof-required mode off for the default PIN-only path,
   or explicitly use the [sender identity workflow](ANDROID_SENDER_IDENTITY.md)
   on both ends. Start reception. If one-shot pairing approval is enabled,
   explicitly approve the next pairing before Send. Keep the PIN private.
4. For automatic discovery, tap **扫描局域网接收器（15 秒）** in Sender and select
   the reachable, explicitly **unverified** address. Keep Receiver foreground.
   In Receiver, open **分享连接信息（不含 PIN）** to view the address/full TLS
   fingerprint for comparison. Discovery does not authorize or connect.
   Alternatively, tap **分享连接信息（不含 PIN）**, select the reachable address, and copy/share
   through your trusted mechanism to the source device. Return to Receiver
   before sending. The displayed SHA-256 is the temporary TLS certificate,
   not the persistent Keystore identity. Restarting reception changes it.
5. For manual entry, paste the exact connection text and tap **检查连接信息**.
   Discovery selection fills/inspects the text and clears prior PIN/confirmation.
   Compare
   the full address and 64-character fingerprint with Receiver. Confirm only
   if both match; enter Receiver's six-digit PIN separately. Never accept a
   ticket from an unknown third party or send your PIN to us.
6. Lower the receiver media volume first. Select 1080p30 (or explicitly uncheck
   for 720p30), tap Send, and keep both apps foreground. Expect moving color
   bars/square and one short tone per second for about 30 seconds. Receiver's
   video-rendered and PCM-buffer counters should increase. The sender's
   counters indicate submission, not successful display or audible quality.
7. Verify normal end releases playback. Repeat at least three times. Also
   cancel midway, leave Sender, stop Receiver, and try an intentionally wrong
   PIN. These should stop/reject; no encrypted-to-clear fallback is supported.

Discovery requires matching new builds with `boot=cert1`, local IPv4 and working
multicast. Entries expire; scan again if needed. Cancel/leave the page to stop
scanning and clear the list; returning never connects automatically. If multiple
addresses appear, compare the receiver's actual Wi-Fi address. Old builds or
filtered multicast can use manual entry; pure IPv6 is not supported by either
ticket path. Report whether discovery, manual entry, or both were tested.

If connection fails: verify both foreground, matching build, current ticket,
PIN, reachable private address and approval mode. If encoding fails at 1080p,
try 720p explicitly and record both results. No silent downgrade is performed.
Do not interpret a successful 720p run as a 1080p pass. No recovery, rotation,
long-run drift correction or background playback/capture is promised here.

## Feedback (no credentials)

Send build name, both device models/Android versions, Wi-Fi type (without SSID
or addresses), selected resolution, success/failure stage, observed motion,
tone/flash relationship, final video/PCM counters, normal-stop/cancel/repeat
results, and the numeric failure code if any. Do **not** send the ticket, PIN,
full fingerprint, IP, personal media or unsanitized logs. A short pass does not
close PLAN's M1 LAN discovery gate or M2 1080p60/one-hour/recovery gates.

## Developer-only reproducibility

Build the shared Rust JNI library with `scripts/build_android_native.sh`, then
run `./gradlew lint test assembleDebug :receiver:assembleDebugAndroidTest
:sender:assembleDebugAndroidTest`. Package verification runs for both APKs.
For local x86_64 instrumentation use `-PframearkPlaybackProbe=true` and place
the matching library under `build/probe-jni/x86_64`. These are not delivery APKs.

`ReferenceReceiverTest#testReceivesSeparateSenderProcess` and
`ReferenceSenderTest#testPinnedReferenceStream` run in separate disposable API
34 emulators. Receiver creates app-private temporary ticket/PIN files; the
test operator forwards that receiver's UDP port on the emulator host and
copies those fixtures into Sender's private files. Sender uses `10.0.2.2`
(emulator host gateway), executes actual encoders/QUIC, and Receiver asserts
at least 880 rendered video frames, 1250 PCM writes and released codecs. The
Sender asserts exactly 900 encoded frames and successful Stop. Files are
deleted in test teardown. `testEncoderCancellationReleasesForNextRun` proves
encoder cancellation followed by another successful run. This is synthetic
emulator integration, not named physical-device compatibility or audible QA.

Windows orchestration is available as `scripts/test_android_reference_sender.ps1`
with required `-ReceiverSerial emulator-5582 -SenderSerial emulator-5584
-Adb <adb-path>`. `-Ui` exercises the actual Sender form, exact-ticket
confirmation, PIN input and Send button instead of calling the bridge directly.
Only `emulator-*` serials are accepted; the script stops only these test apps
and removes their two exact private fixture files plus its UDP forwarding rule.
If emulator Wi-Fi routing sends replies through a different virtual interface
than host UDP redirection, disable Wi-Fi in these disposable emulators **before**
starting the test (emulated Ethernet remains available). This workaround is
not an instruction to change an owner's phone network or a LAN success claim.

Connection failure codes: `-110` timeout, `-111` endpoint, `-112` TLS setup,
`-113` rejected PIN, `-114` rejected approval, `-115` capability negotiation,
`-116` closed transport, `-117` other handshake rejection. Later codes
`-13`/`-14`/`-15` identify Offer/Start/media-open, `-16` invalid media/time,
`-17` media transport, `-19` producer idle, `-20`/`-21` finish/Stop, and `-22`
overall deadline; `-23` identifies rejected/failed capture video reconfiguration.
Native stage codes and static source location are safe to
report; do not include the connection ticket or raw exception/log content.
