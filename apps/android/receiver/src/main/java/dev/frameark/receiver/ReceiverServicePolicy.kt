package dev.frameark.receiver

/** Pure action policy shared by the foreground service and JVM tests. */
object ReceiverServicePolicy {
    const val ACTION_START = "dev.frameark.receiver.action.START"
    const val ACTION_STOP = "dev.frameark.receiver.action.STOP"

    fun normalize(action: String?): String = action ?: ACTION_START

    fun isStart(action: String?): Boolean = normalize(action) == ACTION_START

    fun isStop(action: String?): Boolean = normalize(action) == ACTION_STOP
}
