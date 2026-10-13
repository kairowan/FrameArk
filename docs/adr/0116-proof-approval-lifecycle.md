# ADR 0116: Human-paced FANP proof approval lifecycle

Status: Accepted

## Context

The JNI listener needs to wait long enough for a human decision while remaining
responsive to service shutdown. Replacing an entire handshake every polling
interval prevents a UI from completing that handshake. Cancelling an async
waiter must not leave an actionable request behind.

## Decision

Select the existing bounded handshake/media future against a shutdown poll,
instead of wrapping the complete handshake in a 250ms timeout. Transport phase
timeouts remain in force. The proof queue owns a 30-second monotonic deadline,
enforces a single decision, and removes state on expiry, consumption, stop or
waiter drop. IDs do not wrap across lifecycle restarts. An additive
`nativePeerProofPending` JNI query lets a foreground UI remove stale prompts;
the decision itself still revalidates the request. Rebuild the Kotlin shell and
native library together when using the new query.

## Evidence and limitations

The real loopback test waits 750ms before approval and completes pinned QUIC,
proof verification and capability negotiation. It failed against the previous
listener with `ConnectionClosed`. Rust tests cover expiry, duplicate decisions,
cancelled waiters, stop and stale IDs after restart. These are lifecycle tests,
not physical Android or long-duration playback evidence. The proof mode is
still opt-in and grants only the current session unless a separate explicit
trust flow has enrolled the identity.
