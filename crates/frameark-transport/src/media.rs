//! Bounded length-prefixed QUIC media streams.
//!
//! Control streams carry one small FANP message per bidirectional stream. A
//! media stream is long-lived and carries multiple already-encoded `FAM1`
//! frames, each preceded by a four-byte length. This module owns only stream
//! framing and deadlines; `frameark-native` owns FAM1 validation and decoding.

use std::time::Duration;

use quinn::{RecvStream, SendStream};
use tokio::time::timeout;

use crate::{PairingSession, TransportError};

/// Maximum serialized FAM1 frame accepted by one media stream record.
pub const MAX_MEDIA_FRAME_BYTES: usize = 4 * 1024 * 1024 + 512;
const LENGTH_BYTES: usize = 4;

/// A unidirectional media stream opened by the sender.
pub struct MediaSender {
    send: SendStream,
}

impl MediaSender {
    /// Sends one bounded serialized FAM1 frame within the caller's deadline.
    pub async fn send_frame(
        &mut self,
        frame: &[u8],
        budget: Duration,
    ) -> Result<(), TransportError> {
        if frame.is_empty() || frame.len() > MAX_MEDIA_FRAME_BYTES {
            return Err(TransportError::InvalidFrame);
        }
        timeout(budget, async {
            self.send
                .write_all(&(frame.len() as u32).to_be_bytes())
                .await
                .map_err(|_| TransportError::ConnectionClosed)?;
            self.send
                .write_all(frame)
                .await
                .map_err(|_| TransportError::ConnectionClosed)
        })
        .await
        .map_err(|_| TransportError::Timeout)?
    }

    /// Finishes the stream after the final frame has been sent.
    pub fn finish(mut self) -> Result<(), TransportError> {
        self.send
            .finish()
            .map_err(|_| TransportError::ConnectionClosed)
    }
}

/// A unidirectional media stream accepted by the receiver.
pub struct MediaReceiver {
    receive: RecvStream,
}

impl MediaReceiver {
    /// Receives one length-prefixed FAM1 frame within the caller's deadline.
    pub async fn receive_frame(&mut self, budget: Duration) -> Result<Vec<u8>, TransportError> {
        timeout(budget, async {
            let mut length = [0_u8; LENGTH_BYTES];
            self.receive
                .read_exact(&mut length)
                .await
                .map_err(|_| TransportError::ConnectionClosed)?;
            let length = u32::from_be_bytes(length) as usize;
            if length == 0 || length > MAX_MEDIA_FRAME_BYTES {
                return Err(TransportError::InvalidFrame);
            }
            let mut frame = vec![0_u8; length];
            self.receive
                .read_exact(&mut frame)
                .await
                .map_err(|_| TransportError::ConnectionClosed)?;
            Ok(frame)
        })
        .await
        .map_err(|_| TransportError::Timeout)?
    }
}

impl PairingSession {
    /// Opens a long-lived unidirectional media stream to the peer.
    pub async fn open_media_stream(&self) -> Result<MediaSender, TransportError> {
        let send = self
            .connection
            .open_uni()
            .await
            .map_err(|_| TransportError::ConnectionClosed)?;
        Ok(MediaSender { send })
    }

    /// Accepts the next sender-owned media stream within a bounded deadline.
    pub async fn accept_media_stream(
        &self,
        budget: Duration,
    ) -> Result<MediaReceiver, TransportError> {
        let receive = timeout(budget, self.connection.accept_uni())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|_| TransportError::ConnectionClosed)?;
        Ok(MediaReceiver { receive })
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

    #[tokio::test]
    async fn sends_multiple_bounded_frames_until_fin() {
        let (_server, sender, receiver) = pair().await;
        let mut send = sender.open_media_stream().await.unwrap();
        send.send_frame(b"FAM1-first", Duration::from_secs(2))
            .await
            .unwrap();
        let mut receive = receiver
            .accept_media_stream(Duration::from_secs(2))
            .await
            .unwrap();
        send.send_frame(b"FAM1-second", Duration::from_secs(2))
            .await
            .unwrap();
        send.finish().unwrap();
        assert_eq!(
            receive.receive_frame(Duration::from_secs(2)).await.unwrap(),
            b"FAM1-first"
        );
        assert_eq!(
            receive.receive_frame(Duration::from_secs(2)).await.unwrap(),
            b"FAM1-second"
        );
        assert_eq!(
            receive.receive_frame(Duration::from_secs(2)).await,
            Err(TransportError::ConnectionClosed)
        );
    }

    #[tokio::test]
    async fn rejects_empty_and_oversized_frames_before_writing() {
        let (_server, sender, _receiver) = pair().await;
        let mut send = sender.open_media_stream().await.unwrap();
        assert_eq!(
            send.send_frame(&[], Duration::from_secs(1)).await,
            Err(TransportError::InvalidFrame)
        );
        assert_eq!(
            send.send_frame(&vec![0; MAX_MEDIA_FRAME_BYTES + 1], Duration::from_secs(1))
                .await,
            Err(TransportError::InvalidFrame)
        );
    }
}
