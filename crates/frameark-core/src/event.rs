use crate::{Device, SessionTransition};

/// Severity used for redacted diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EventLevel {
    /// Informational lifecycle event.
    Info,
    /// Recoverable problem or degraded capability.
    Warn,
    /// Operation failed or a session was closed unexpectedly.
    Error,
}

/// A diagnostic event safe to expose to platform logs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticEvent {
    /// Event severity.
    pub level: EventLevel,
    /// Stable event code.
    pub code: String,
    /// Redacted human-readable summary.
    pub message: String,
}

impl DiagnosticEvent {
    /// Creates a diagnostic event. The caller must provide a redacted message.
    pub fn new(level: EventLevel, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Events emitted by the shared core to platform adapters and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreEvent {
    /// A device was discovered or its advertised data changed.
    DeviceDiscovered { device: Device },
    /// A session lifecycle transition completed.
    SessionTransition(SessionTransition),
    /// A redacted diagnostic event.
    Diagnostic(DiagnosticEvent),
}
