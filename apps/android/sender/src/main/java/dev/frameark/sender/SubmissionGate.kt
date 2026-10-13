package dev.frameark.sender

/** Confirmation is tied to the exact inspected text, never a mutable form. */
class SubmissionGate {
    private var inspected: String? = null
    fun inspect(ticket: String, valid: Boolean) { inspected = ticket.takeIf { valid } }
    fun permits(ticket: String, pin: String, confirmed: Boolean): Boolean =
        confirmed && inspected == ticket && pin.length == 6 && pin.all { it in '0'..'9' }
}
