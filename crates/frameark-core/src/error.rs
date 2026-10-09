use std::error::Error;
use std::fmt::{Display, Formatter};

/// Stable high-level categories used by diagnostics and platform adapters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    /// A caller supplied a value that cannot be accepted.
    InvalidArgument,
    /// An operation conflicts with the current lifecycle state.
    InvalidState,
    /// The requested capability is not implemented or available.
    Unsupported,
    /// An operation exceeded its configured time budget.
    Timeout,
    /// A transport or discovery operation failed.
    Transport,
    /// A media configuration or frame operation failed.
    Media,
    /// Authentication, authorization, or trust policy rejected an operation.
    Security,
    /// An unexpected internal invariant failed.
    Internal,
}

impl Display for ErrorKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::InvalidArgument => "invalid_argument",
            Self::InvalidState => "invalid_state",
            Self::Unsupported => "unsupported",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::Media => "media",
            Self::Security => "security",
            Self::Internal => "internal",
        };
        formatter.write_str(name)
    }
}

/// A redaction-safe error value shared across the core and platform boundaries.
///
/// Callers must pass summaries rather than raw packets, URLs, credentials, or
/// user media in `message`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameArkError {
    kind: ErrorKind,
    code: String,
    message: String,
}

impl FrameArkError {
    /// Creates an error with a stable code and a diagnostic-safe message.
    pub fn new(kind: ErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Creates an invalid-argument error.
    pub fn invalid_argument(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidArgument, code, message)
    }

    /// Creates an invalid-state error.
    pub fn invalid_state(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidState, code, message)
    }

    /// Creates an unsupported-capability error.
    pub fn unsupported(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, code, message)
    }

    /// Returns the broad error category.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the redaction-safe human-readable summary.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for FrameArkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({}): {}", self.kind, self.code, self.message)
    }
}

impl Error for FrameArkError {}

/// The common result type used by FrameArk core APIs.
pub type Result<T> = std::result::Result<T, FrameArkError>;
