package dev.frameark.sender

import org.junit.Assert.*
import org.junit.Test

class SenderPolicyTest {
    @Test fun confirmationBindsExactInspectedTicketAndAsciiPin() {
        val gate = SubmissionGate()
        assertFalse(gate.permits("a", "123456", true))
        gate.inspect("a", true)
        assertTrue(gate.permits("a", "123456", true))
        assertFalse(gate.permits("b", "123456", true))
        assertFalse(gate.permits("a", "123456", false))
        assertFalse(gate.permits("a", "１２３４５６", true))
        assertFalse(gate.permits("a", "12345", true))
        gate.inspect("a", false)
        assertFalse(gate.permits("a", "123456", true))
    }
    @Test fun annexBRepeatsConfigurationOnlyForKeyframes() {
        val config = byteArrayOf(0, 0, 0, 1, 0x67)
        val payload = byteArrayOf(0, 0, 1, 0x65, 42)
        assertArrayEquals(config + payload, AnnexB.accessUnit(config, payload, true))
        assertSame(payload, AnnexB.accessUnit(config, payload, false))
        for (bad in listOf(byteArrayOf(), byteArrayOf(0, 0, 0, 5, 0x65), ByteArray(4 * 1024 * 1024 + 1))) {
            assertThrows(IllegalArgumentException::class.java) { AnnexB.accessUnit(config, bad, true) }
        }
        assertThrows(IllegalArgumentException::class.java) { AnnexB.accessUnit(ByteArray(0), payload, true) }
    }
}
