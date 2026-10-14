# Android Sender identity proof · Experimental

Use matching Sender/Receiver preview APKs on your own Android 8+ ARM64 devices.
No ADB, key export or private media upload is needed. This optional flow applies
to both the reference test stream and Android 10+ screen sharing.

1. In Sender tap **加载或创建发送端身份（私钥不导出）**, then check **本次使用发送端身份签名**.
   Record nothing publicly: the shown sender ID/full fingerprint is a persistent
   pseudonymous identifier. The private key remains in this app's Keystore.
2. In Receiver, stop reception, tap **Require verified sender proof**, then
   start reception again. If the separate pre-PIN approval gate is also enabled,
   arm it explicitly. Keep Receiver foreground for the proof prompt.
3. Discover/select or manually paste the current receiver ticket. Compare its
   complete TLS fingerprint with Receiver, confirm, then enter the PIN. This
   TLS fingerprint is **not** the sender identity fingerprint from step 1.
4. Start the reference stream or explicitly authorize screen sharing. When
   Receiver displays the verified sender prompt, compare the full sender ID and
   fingerprint with Sender. Approve only if both match. Decide within 30 seconds.
   Wrong identity, Reject, timeout or cancellation must prevent streaming.
5. Repeat approval, rejection and cancellation. Reload Sender identity: its
   ID/fingerprint should remain unchanged without uninstall/data reset. Send
   only device models/OS/build and pass/fail/numeric error, never ticket/PIN,
   fingerprints, raw logs or private keys.

Approval is **one session**, not “always allow.” To remember a sender using the
existing experimental trust settings, first verify it with the above flow,
stop Receiver, and explicitly enter the verified sender ID and full fingerprint
with a local label. This manual enrollment is security-sensitive; do not use
unverified discovery data. Restart reception so Rust uses the new snapshot.
An exact fresh signed match may skip the proof prompt but still requires PIN.
Stop/revoke/restart to require approval again. Revoking does not terminate a
session that is already running; live revocation is not implemented.

Keys lost after uninstall/reset cannot be silently restored. Do not delete app
data to fix a key or signature error without deciding whether old trust data
must be retained. OS storage/hardware-backed protection differs by device.
Signing failure never rotates keys, bypasses proof, or starts PIN-only mode.
To intentionally test legacy mode, disable both sides' proof settings for a
new session; it does not satisfy the identity acceptance test.

## Disposable emulator test (developer only)

Run `dev.frameark.sender.SenderIdentityTest` with matching test APK and x86_64
JNI library, only on an agent-owned disposable emulator. It provisions a real
Android Keystore P-256 key, asserts the private key is nonexportable, reloads
the same public key, and drives production Receiver JNI + Sender JNI through
real pinned QUIC in the **same instrumentation process**. It checks approve,
deny, cancel, stopped-state explicit enrollment, matching reconnect without a
prompt, revoke/restart prompt, and late signature rejection. No credentials are
written to files or logs. This is not two-device LAN, Receiver UI, decoding,
physical hardware security or M2 milestone evidence.
