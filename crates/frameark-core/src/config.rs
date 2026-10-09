use std::time::Duration;

use crate::{FrameArkError, Result};

const MAX_CONTROL_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// Runtime limits shared by protocol and platform adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreConfig {
    /// Maximum accepted size of one control message.
    pub max_control_message_bytes: usize,
    /// Timeout for establishing or refreshing a transport connection.
    pub connect_timeout: Duration,
    /// Timeout after which an idle session may be closed.
    pub idle_timeout: Duration,
    /// Maximum number of queued core events before backpressure is reported.
    pub max_pending_events: usize,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            max_control_message_bytes: 1024 * 1024,
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(30),
            max_pending_events: 256,
        }
    }
}

impl CoreConfig {
    /// Validates bounded resource and timeout settings.
    pub fn validate(&self) -> Result<()> {
        if self.max_control_message_bytes == 0
            || self.max_control_message_bytes > MAX_CONTROL_MESSAGE_BYTES
        {
            return Err(FrameArkError::invalid_argument(
                "config.invalid_control_message_limit",
                "control message limit must be between 1 byte and 16 MiB",
            ));
        }
        if self.connect_timeout.is_zero() || self.idle_timeout.is_zero() {
            return Err(FrameArkError::invalid_argument(
                "config.zero_timeout",
                "connection and idle timeouts must be non-zero",
            ));
        }
        if self.max_pending_events == 0 {
            return Err(FrameArkError::invalid_argument(
                "config.zero_event_limit",
                "event queue limit must be non-zero",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(CoreConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_unbounded_message_limit() {
        let config = CoreConfig {
            max_control_message_bytes: MAX_CONTROL_MESSAGE_BYTES + 1,
            ..CoreConfig::default()
        };
        assert_eq!(
            config.validate().expect_err("limit must be bounded").code(),
            "config.invalid_control_message_limit"
        );
    }
}
