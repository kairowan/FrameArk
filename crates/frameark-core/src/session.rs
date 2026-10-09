use std::fmt::{Display, Formatter};

use crate::{DeviceId, FrameArkError, Result, SessionId};

/// The unified lifecycle shared by every protocol adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionState {
    /// No connection attempt has started.
    Idle,
    /// The transport connection is being established.
    Connecting,
    /// Pairing or authentication is in progress.
    Authenticating,
    /// Capabilities and media tracks are being negotiated.
    Negotiating,
    /// Renderers and buffers are being prepared.
    Preparing,
    /// Media is flowing.
    Streaming,
    /// A stream configuration is changing.
    Reconfiguring,
    /// The sender is finishing and buffers are draining.
    Draining,
    /// The connection is recovering after a recoverable interruption.
    Recovering,
    /// The session has released its resources and cannot be reused.
    Closed,
}

impl Display for SessionState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Authenticating => "authenticating",
            Self::Negotiating => "negotiating",
            Self::Preparing => "preparing",
            Self::Streaming => "streaming",
            Self::Reconfiguring => "reconfiguring",
            Self::Draining => "draining",
            Self::Recovering => "recovering",
            Self::Closed => "closed",
        };
        formatter.write_str(name)
    }
}

/// A validated session lifecycle transition.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SessionTransition {
    /// Session that changed state.
    pub session_id: SessionId,
    /// State before the transition.
    pub from: SessionState,
    /// State after the transition.
    pub to: SessionState,
}

/// A protocol-independent media session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    id: SessionId,
    device_id: DeviceId,
    state: SessionState,
}

impl Session {
    /// Creates a new session in the idle state.
    pub const fn new(id: SessionId, device_id: DeviceId) -> Self {
        Self {
            id,
            device_id,
            state: SessionState::Idle,
        }
    }

    /// Returns the session identifier.
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// Returns the target device identifier.
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> SessionState {
        self.state
    }

    /// Applies a lifecycle transition after checking the shared state machine.
    pub fn transition(&mut self, next: SessionState) -> Result<SessionTransition> {
        if !is_allowed(self.state, next) {
            return Err(FrameArkError::invalid_state(
                "session.invalid_transition",
                format!("cannot transition from {} to {}", self.state, next),
            ));
        }
        let transition = SessionTransition {
            session_id: self.id.clone(),
            from: self.state,
            to: next,
        };
        self.state = next;
        Ok(transition)
    }
}

const fn is_allowed(from: SessionState, to: SessionState) -> bool {
    use SessionState::{
        Authenticating, Closed, Connecting, Draining, Idle, Negotiating, Preparing, Reconfiguring,
        Recovering, Streaming,
    };

    matches!(
        (from, to),
        (Idle, Connecting | Closed)
            | (Connecting, Authenticating | Recovering | Closed)
            | (Authenticating, Negotiating | Recovering | Closed)
            | (Negotiating, Preparing | Recovering | Closed)
            | (Preparing, Streaming | Recovering | Closed)
            | (Streaming, Reconfiguring | Draining | Recovering | Closed)
            | (Reconfiguring, Preparing | Streaming | Recovering | Closed)
            | (Draining, Closed | Recovering)
            | (
                Recovering,
                Connecting | Authenticating | Negotiating | Closed
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(
            SessionId::try_from("session-1").expect("valid session id"),
            DeviceId::try_from("device-1").expect("valid device id"),
        )
    }

    #[test]
    fn follows_the_native_session_sequence() {
        let mut session = session();
        for state in [
            SessionState::Connecting,
            SessionState::Authenticating,
            SessionState::Negotiating,
            SessionState::Preparing,
            SessionState::Streaming,
            SessionState::Draining,
            SessionState::Closed,
        ] {
            session.transition(state).expect("valid transition");
        }
        assert_eq!(session.state(), SessionState::Closed);
    }

    #[test]
    fn rejects_skipping_negotiation() {
        let mut session = session();
        let error = session
            .transition(SessionState::Streaming)
            .expect_err("idle cannot stream");
        assert_eq!(error.code(), "session.invalid_transition");
        assert_eq!(error.kind(), crate::ErrorKind::InvalidState);
    }

    #[test]
    fn permits_recovery_and_releases_terminal_state() {
        let mut session = session();
        session
            .transition(SessionState::Connecting)
            .expect("connect");
        session
            .transition(SessionState::Recovering)
            .expect("recover");
        session.transition(SessionState::Connecting).expect("retry");
        session.transition(SessionState::Closed).expect("close");
        assert!(session.transition(SessionState::Idle).is_err());
    }
}
