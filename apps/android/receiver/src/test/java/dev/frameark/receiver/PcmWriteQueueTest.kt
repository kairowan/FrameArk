package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class PcmWriteQueueTest {
    @Test fun zero_and_partial_writes_preserve_owned_bytes_in_order() {
        val queue = PcmWriteQueue()
        val input = byteArrayOf(1, 2, 3, 4)
        queue.append(input)
        input.fill(0)
        queue.drain { _, _, _ -> 0 }
        val written = mutableListOf<Byte>()
        queue.drain { bytes, offset, _ -> written.add(bytes[offset]); 1 }
        assertEquals(listOf<Byte>(1, 2, 3, 4), written)
        queue.drain { _, _, _ -> error("queue must be empty") }
    }

    @Test fun capacity_failure_and_reset_are_bounded() {
        val queue = PcmWriteQueue()
        queue.append(ByteArray(256 * 1024))
        assertThrows(IllegalArgumentException::class.java) { queue.append(byteArrayOf(1)) }
        queue.clear()
        repeat(64) { queue.append(byteArrayOf(1)) }
        assertThrows(IllegalArgumentException::class.java) { queue.append(byteArrayOf(1)) }
        var calls = 0
        queue.drain { _, _, length -> calls++; length }
        assertEquals(16, calls)
        queue.clear()
        queue.drain { _, _, _ -> error("reset retains no PCM") }
    }

    @Test fun invalid_platform_write_fails_explicitly() {
        val queue = PcmWriteQueue()
        queue.append(byteArrayOf(1))
        assertThrows(IllegalArgumentException::class.java) { queue.drain { _, _, _ -> -1 } }
        assertThrows(IllegalArgumentException::class.java) { queue.drain { _, _, _ -> 2 } }
    }
}
