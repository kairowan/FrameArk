package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReceiverServicePolicyTest {
    @Test
    fun missing_action_defaults_to_start() {
        assertEquals(ReceiverServicePolicy.ACTION_START, ReceiverServicePolicy.normalize(null))
        assertTrue(ReceiverServicePolicy.isStart(null))
    }

    @Test
    fun stop_action_is_terminal() {
        assertTrue(ReceiverServicePolicy.isStop(ReceiverServicePolicy.ACTION_STOP))
        assertEquals(
            ReceiverServicePolicy.ACTION_START,
            ReceiverServicePolicy.normalize(ReceiverServicePolicy.ACTION_START),
        )
    }
}
