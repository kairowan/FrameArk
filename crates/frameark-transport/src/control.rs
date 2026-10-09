//! Bounded, authenticated, single-request control streams.

use std::time::Duration;

use quinn::{Connection, SendStream};
use tokio::time::timeout;

use crate::{MAX_FRAME_PAYLOAD, PairingSession, TransportError, read_frame, write_frame};

/// A protocol adapter's post-handshake control message (types 16 through 127).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlMessage {
    kind: u8,
    payload: Vec<u8>,
}

impl ControlMessage {
    /// Validates type and size before copying a caller-supplied payload.
    pub fn new(kind: u8, payload: &[u8]) -> Result<Self, TransportError> {
        if !(16..=127).contains(&kind) || payload.len() > MAX_FRAME_PAYLOAD {
            return Err(TransportError::InvalidFrame);
        }
        Ok(Self {
            kind,
            payload: payload.to_vec(),
        })
    }

    /// Returns the protocol adapter's message type.
    pub const fn kind(&self) -> u8 {
        self.kind
    }

    /// Returns the bounded payload, without logging it.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Owns a request until the receiver acknowledges it; dropping it closes the connection.
pub struct PendingControl {
    request: ControlMessage,
    send: SendStream,
    guard: CloseOnFailure,
}

impl PendingControl {
    /// Borrows the authenticated request.
    pub fn request(&self) -> &ControlMessage {
        &self.request
    }

    /// Sends exactly one response and waits for transport acknowledgement.
    /// This avoids a sleep-based race when a terminal response is followed by close.
    pub async fn respond(
        mut self,
        response: ControlMessage,
        budget: Duration,
    ) -> Result<(), TransportError> {
        timeout(budget, async {
            write_frame(&mut self.send, response.kind, &response.payload).await?;
            match self.send.stopped().await {
                Ok(None) => Ok(()),
                _ => Err(TransportError::ConnectionClosed),
            }
        })
        .await
        .map_err(|_| TransportError::Timeout)??;
        self.guard.disarm();
        Ok(())
    }
}

impl PairingSession {
    /// Performs one request/response within a total deadline. Failure or cancellation
    /// closes the session; mutability prevents pipelining through this API.
    pub async fn request_control(
        &mut self,
        request: ControlMessage,
        budget: Duration,
    ) -> Result<ControlMessage, TransportError> {
        let mut guard = CloseOnFailure::new(self.connection.clone());
        let response = timeout(budget, async {
            let (mut send, mut recv) = self
                .connection
                .open_bi()
                .await
                .map_err(|_| TransportError::ConnectionClosed)?;
            write_frame(&mut send, request.kind, &request.payload).await?;
            let frame = read_frame(&mut recv).await?;
            ControlMessage::new(frame.kind, &frame.payload)
        })
        .await
        .map_err(|_| TransportError::Timeout)??;
        guard.disarm();
        Ok(response)
    }

    /// Waits for a complete bounded request. Dropping the returned request or
    /// cancelling this future closes the session, so no half-read state is reused.
    pub async fn accept_control(
        &mut self,
        budget: Duration,
    ) -> Result<PendingControl, TransportError> {
        let guard = CloseOnFailure::new(self.connection.clone());
        let (request, send) = timeout(budget, async {
            let (send, mut recv) = self
                .connection
                .accept_bi()
                .await
                .map_err(|_| TransportError::ConnectionClosed)?;
            let frame = read_frame(&mut recv).await?;
            Ok((ControlMessage::new(frame.kind, &frame.payload)?, send))
        })
        .await
        .map_err(|_| TransportError::Timeout)??;
        Ok(PendingControl {
            request,
            send,
            guard,
        })
    }
}

pub(crate) struct CloseOnFailure(Option<Connection>);

impl CloseOnFailure {
    pub(crate) fn new(connection: Connection) -> Self {
        Self(Some(connection))
    }
    pub(crate) fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for CloseOnFailure {
    fn drop(&mut self) {
        if let Some(connection) = &self.0 {
            connection.close(1u32.into(), b"control operation ended");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PairingClient, PairingCode, PairingServer};

    async fn pair() -> (PairingServer, PairingSession, PairingSession) {
        let server = PairingServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            "localhost",
            PairingCode::parse("123456").unwrap(),
        )
        .unwrap();
        let (receiver, sender) = tokio::join!(
            server.accept_pairing(Duration::from_secs(5)),
            PairingClient::connect(
                server.local_addr().unwrap(),
                "localhost",
                server.certificate_der(),
                PairingCode::parse("123456").unwrap(),
                Duration::from_secs(5)
            ),
        );
        (server, sender.unwrap(), receiver.unwrap())
    }

    #[test]
    fn message_bounds_reject_handshake_types_and_oversize() {
        assert!(ControlMessage::new(1, &[]).is_err());
        assert!(ControlMessage::new(128, &[]).is_err());
        assert!(ControlMessage::new(16, &[0; MAX_FRAME_PAYLOAD + 1]).is_err());
        assert!(ControlMessage::new(16, &[0; MAX_FRAME_PAYLOAD]).is_ok());
    }

    #[tokio::test]
    async fn response_is_delivered_before_terminal_close() {
        let (_server, mut sender, mut receiver) = pair().await;
        let task = tokio::spawn(async move {
            let pending = receiver
                .accept_control(Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(pending.request().payload(), b"test");
            pending
                .respond(
                    ControlMessage::new(17, b"ack").unwrap(),
                    Duration::from_secs(2),
                )
                .await
                .unwrap();
            receiver.close();
        });
        let response = sender
            .request_control(
                ControlMessage::new(16, b"test").unwrap(),
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert_eq!(response.payload(), b"ack");
        task.await.unwrap();
    }

    #[tokio::test]
    async fn request_timeout_closes_the_connection() {
        let (_server, mut sender, _receiver) = pair().await;
        let result = sender
            .request_control(
                ControlMessage::new(16, b"test").unwrap(),
                Duration::from_millis(30),
            )
            .await;
        assert_eq!(result, Err(TransportError::Timeout));
        assert!(sender.connection.close_reason().is_some());
    }

    #[tokio::test]
    async fn dropped_request_cancels_the_exchange() {
        let (_server, mut sender, mut receiver) = pair().await;
        let task = tokio::spawn(async move {
            let pending = receiver
                .accept_control(Duration::from_secs(2))
                .await
                .unwrap();
            drop(pending);
            assert!(receiver.connection.close_reason().is_some());
        });
        assert!(
            sender
                .request_control(
                    ControlMessage::new(16, b"test").unwrap(),
                    Duration::from_secs(2)
                )
                .await
                .is_err()
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn trailing_bytes_are_rejected() {
        let (_server, sender, mut receiver) = pair().await;
        let (mut send, _recv) = sender.connection.open_bi().await.unwrap();
        send.write_all(b"FANP\x01\x10\x00\x00trailing")
            .await
            .unwrap();
        send.finish().unwrap();
        assert!(matches!(
            receiver.accept_control(Duration::from_secs(2)).await,
            Err(TransportError::InvalidFrame)
        ));
    }

    #[test]
    fn code_debug_is_redacted() {
        let secret = PairingCode::parse("123456").unwrap();
        assert_eq!(format!("{secret:?}"), "PairingCode([REDACTED])");
    }
}
