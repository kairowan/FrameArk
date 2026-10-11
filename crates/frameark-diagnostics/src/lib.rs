//! Bounded, redaction-safe diagnostics for FrameArk sessions.
//!
//! This crate records only caller-supplied summaries. It never stores raw
//! packets, URLs, credentials, device identifiers, or media and it performs
//! a second pass that removes control characters and bounds text fields.

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};

use frameark_core::{DiagnosticEvent, ErrorKind, EventLevel, FrameArkError, SessionTransition};

/// Maximum number of records retained by one recorder.
pub const MAX_RECORDS: usize = 512;
/// Maximum code length retained in one record.
pub const MAX_CODE_BYTES: usize = 96;
/// Maximum message length retained in one record.
pub const MAX_MESSAGE_BYTES: usize = 512;

/// Errors raised while creating a recorder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticsError {
    /// Capacity must be non-zero and within the bounded maximum.
    InvalidCapacity,
}

impl Display for DiagnosticsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("diagnostics capacity must be between 1 and MAX_RECORDS")
    }
}

impl std::error::Error for DiagnosticsError {}

/// One retained, redaction-safe diagnostic record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRecord {
    /// Monotonic sequence assigned by the recorder.
    pub sequence: u64,
    /// Caller-provided elapsed time since the session or process started.
    pub elapsed_ms: u64,
    /// Severity of the event.
    pub level: EventLevel,
    /// Bounded stable event code.
    pub code: String,
    /// Bounded, control-character-free summary.
    pub message: String,
}

/// An in-memory ring of bounded diagnostic records.
#[derive(Clone, Debug)]
pub struct DiagnosticRecorder {
    capacity: usize,
    next_sequence: u64,
    records: VecDeque<DiagnosticRecord>,
}

impl DiagnosticRecorder {
    /// Creates a recorder with an explicit bounded capacity.
    pub fn new(capacity: usize) -> Result<Self, DiagnosticsError> {
        if capacity == 0 || capacity > MAX_RECORDS {
            return Err(DiagnosticsError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            next_sequence: 0,
            records: VecDeque::with_capacity(capacity),
        })
    }

    /// Returns the configured record capacity.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Records a caller-supplied redaction-safe event.
    pub fn record(&mut self, elapsed_ms: u64, event: &DiagnosticEvent) {
        self.push(elapsed_ms, event.level, &event.code, &event.message);
    }

    /// Records a session transition without retaining its session identifier.
    pub fn record_transition(&mut self, elapsed_ms: u64, transition: &SessionTransition) {
        let message = format!("{} -> {}", transition.from, transition.to);
        self.push(elapsed_ms, EventLevel::Info, "session.transition", &message);
    }

    /// Records a core error without retaining identifiers or source details.
    pub fn record_error(&mut self, elapsed_ms: u64, error: &FrameArkError) {
        self.push(
            elapsed_ms,
            match error.kind() {
                ErrorKind::InvalidArgument | ErrorKind::InvalidState | ErrorKind::Unsupported => {
                    EventLevel::Warn
                }
                ErrorKind::Timeout
                | ErrorKind::Transport
                | ErrorKind::Media
                | ErrorKind::Security
                | ErrorKind::Internal => EventLevel::Error,
            },
            error.code(),
            error.message(),
        );
    }

    /// Returns records in chronological sequence order.
    pub fn records(&self) -> impl Iterator<Item = &DiagnosticRecord> {
        self.records.iter()
    }

    /// Returns a snapshot that can be handed to a platform exporter.
    pub fn snapshot(&self) -> Vec<DiagnosticRecord> {
        self.records.iter().cloned().collect()
    }

    /// Renders a deterministic tab-separated support summary.
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        for record in &self.records {
            use std::fmt::Write as _;
            let _ = writeln!(
                output,
                "{}\t{}\t{}\t{}\t{}",
                record.sequence,
                record.elapsed_ms,
                level_name(record.level),
                record.code,
                record.message
            );
        }
        output
    }

    fn push(&mut self, elapsed_ms: u64, level: EventLevel, code: &str, message: &str) {
        let record = DiagnosticRecord {
            sequence: self.next_sequence,
            elapsed_ms,
            level,
            code: sanitize(code, MAX_CODE_BYTES),
            message: sanitize(message, MAX_MESSAGE_BYTES),
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }
}

fn level_name(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Info => "info",
        EventLevel::Warn => "warn",
        EventLevel::Error => "error",
    }
}

fn sanitize(value: &str, max_bytes: usize) -> String {
    let mut output = String::new();
    let mut bytes = 0;
    for character in value.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        let width = character.len_utf8();
        if bytes + width > max_bytes {
            break;
        }
        output.push(character);
        bytes += width;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_rejects_unbounded_capacity_and_evicts_oldest() {
        assert!(matches!(
            DiagnosticRecorder::new(0),
            Err(DiagnosticsError::InvalidCapacity)
        ));
        assert!(matches!(
            DiagnosticRecorder::new(MAX_RECORDS + 1),
            Err(DiagnosticsError::InvalidCapacity)
        ));
        let mut recorder = DiagnosticRecorder::new(2).unwrap();
        for index in 0..3 {
            recorder.record(
                index,
                &DiagnosticEvent::new(EventLevel::Info, format!("event.{index}"), "ok"),
            );
        }
        let records = recorder.snapshot();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sequence, 1);
        assert_eq!(records[1].sequence, 2);
    }

    #[test]
    fn recorder_sanitizes_and_bounds_text() {
        let mut recorder = DiagnosticRecorder::new(1).unwrap();
        recorder.record(
            7,
            &DiagnosticEvent::new(
                EventLevel::Warn,
                "code\nwith-control",
                "message\twith\rcontrol and 文字",
            ),
        );
        let record = recorder.snapshot().pop().unwrap();
        assert!(!record.code.chars().any(char::is_control));
        assert!(!record.message.chars().any(char::is_control));
        assert_eq!(
            recorder.render_text(),
            "0\t7\twarn\tcode with-control\tmessage with control and 文字\n"
        );
    }

    #[test]
    fn transition_and_error_exports_omit_session_identity() {
        let mut recorder = DiagnosticRecorder::new(4).unwrap();
        let transition = SessionTransition {
            session_id: frameark_core::SessionId::try_from("secret-session").unwrap(),
            from: frameark_core::SessionState::Connecting,
            to: frameark_core::SessionState::Authenticating,
        };
        recorder.record_transition(10, &transition);
        let error = FrameArkError::new(
            ErrorKind::Security,
            "trust.rejected",
            "peer rejected by policy",
        );
        recorder.record_error(11, &error);
        let rendered = recorder.render_text();
        assert!(rendered.contains("connecting -> authenticating"));
        assert!(!rendered.contains("secret-session"));
        assert!(rendered.contains("error\ttrust.rejected"));
    }
}
