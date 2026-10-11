package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Test

class ReceiverIdentityCoordinatorTest {
    private val identity = DeviceIdentity(
        alias = "frameark-device",
        publicKeyFingerprint = "00".repeat(32).chunked(2).joinToString(":"),
    )

    @Test
    fun initializes_once_and_keeps_the_redacted_identity() {
        var loads = 0
        val coordinator = ReceiverIdentityCoordinator {
            loads += 1
            identity
        }

        assertEquals(ReceiverIdentityCoordinator.State.Ready(identity), coordinator.initialize())
        assertEquals(ReceiverIdentityCoordinator.State.Ready(identity), coordinator.initialize())
        assertEquals(1, loads)
    }

    @Test
    fun classifies_keystore_and_identity_failures_without_exposing_messages() {
        val unavailable = ReceiverIdentityCoordinator {
            error("keystore path")
        }
        assertEquals(
            ReceiverIdentityCoordinator.State.Failed(
                ReceiverIdentityCoordinator.FailureReason.KEYSTORE_UNAVAILABLE,
            ),
            unavailable.initialize(),
        )

        val invalid = ReceiverIdentityCoordinator {
            DeviceIdentity("FrameArk", identity.publicKeyFingerprint)
        }
        assertEquals(
            ReceiverIdentityCoordinator.State.Failed(
                ReceiverIdentityCoordinator.FailureReason.INVALID_IDENTITY,
            ),
            invalid.initialize(),
        )
    }
}
