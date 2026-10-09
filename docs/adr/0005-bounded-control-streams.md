# ADR 0005: Bounded authenticated control streams

- Status: Accepted
- Date: 2026-10-09

Protocol adapters need request/response I/O without exposing raw QUIC handles.
`ControlMessage` reserves types 16..127, preserves the 256-byte payload bound,
and requires exactly one frame and FIN per stream. `PendingControl` owns the
response lifetime; dropping it, cancellation, or timeout closes the connection.
Responses wait for bounded transport acknowledgement, not an arbitrary delay.
Transport receipt is not proof of application acceptance: upper protocols must
validate response IDs, states, and selected parameters themselves.

This API is Experimental. It intentionally disallows multiplexing through a
single mutable session handle. It provides neither media datagrams nor replay
protection across connections; protocol adapters must sequence requests.
