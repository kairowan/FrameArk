package dev.frameark.receiver

/** Test APK only: binds existing production Receiver JNI exports in the fixture
 * process. No exported component, test native hook, or credential file is added. */
class FrameArkNative {
    external fun nativeSetDeviceIdentity(alias: String, fingerprint: String): Int
    external fun nativeSetPeerProofRequired(required: Boolean): Int
    external fun nativeStartReceiver(): Int
    external fun nativeStopReceiver(): Int
    external fun nativeReceiverPairingCode(): Int
    external fun nativePairingTicket(address: String): String?
    external fun nativePollPeerProof(): ByteArray?
    external fun nativeDecidePeerProof(id: Long, approved: Boolean): Int
    external fun nativeTrustPeerPublicKey(id: String, key: ByteArray, label: String): Int
    external fun nativeRevokePeer(id: String): Int
}
