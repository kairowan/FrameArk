package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Test

class DeviceIdentityPolicyTest {
    @Test
    fun formats_a_stable_sha256_public_key_fingerprint() {
        assertEquals(
            "9f:64:a7:47:e1:b9:7f:13:1f:ab:b6:b4:47:29:6c:9b:6f:02:01:e7:9f:b3:c5:35:6e:6c:77:e8:9b:6a:80:6a",
            DeviceIdentityPolicy.fingerprint(byteArrayOf(1, 2, 3, 4)),
        )
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejects_uppercase_aliases() {
        DeviceIdentityPolicy.validateAlias("FrameArk")
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejects_empty_public_keys() {
        DeviceIdentityPolicy.fingerprint(byteArrayOf())
    }
}
