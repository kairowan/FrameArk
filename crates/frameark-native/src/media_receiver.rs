//! Native media-stream orchestration for one negotiated FANP session.
//!
//! [`frameark_transport`] owns QUIC stream framing and [`media_wire`] owns the
//! bounded FAM1 representation. This module is the small bridge between those
//! layers: it decodes each received access unit, routes it to the configured
//! platform renderers, records redaction-safe counters, and always releases
//! the renderers when the stream ends or fails.

use std::time::Duration;

use frameark_core::{ErrorKind, FrameArkError, Result, SessionState};
use frameark_transport::{MediaReceiver, TransportError};

use crate::media_session::MediaSession;
use crate::media_wire;
use frameark_api::{AudioRenderer, VideoRenderer};

/// Bounded counters for one received media stream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MediaStreamReport {
    /// Number of successfully decoded and rendered FAM1 frames.
    pub frames: u64,
    /// Number of video access units rendered.
    pub video_frames: u64,
    /// Number of audio access units rendered.
    pub audio_frames: u64,
    /// Total encoded payload bytes handed to platform renderers.
    pub payload_bytes: u64,
}

fn transport_error(error: TransportError) -> FrameArkError {
    let kind = if error == TransportError::Timeout {
        ErrorKind::Timeout
    } else {
        ErrorKind::Transport
    };
    FrameArkError::new(
        kind,
        "fanp.media_transport",
        "media stream transport failed",
    )
}

/// Owns a transport media stream and its platform renderer session.
pub struct NativeMediaReceiver<V: VideoRenderer, A: AudioRenderer> {
    stream: MediaReceiver,
    session: MediaSession<V, A>,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::{SessionOffer, media_wire};
    use frameark_api::{AudioFrame, VideoFrame};
    use frameark_core::{AudioConfig, MediaCodec, TimeBase, TrackId, VideoConfig};
    use frameark_media::{
        AudioPacket, MediaPacket, MediaTimestamp, VideoFrame as EncodedVideoFrame,
    };
    use frameark_transport::{PairingClient, PairingCode, PairingServer};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Counts {
        video: usize,
        audio: usize,
        resets: usize,
    }

    struct FakeVideo(Arc<Mutex<Counts>>);

    impl VideoRenderer for FakeVideo {
        type Error = ();

        fn configure(&mut self, _config: VideoConfig) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn render(&mut self, _frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
            self.0.lock().unwrap().video += 1;
            Ok(())
        }

        fn reset(&mut self) -> std::result::Result<(), Self::Error> {
            self.0.lock().unwrap().resets += 1;
            Ok(())
        }
    }

    struct FakeAudio(Arc<Mutex<Counts>>);

    impl AudioRenderer for FakeAudio {
        type Error = ();

        fn configure(&mut self, _config: AudioConfig) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn render(&mut self, _frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
            self.0.lock().unwrap().audio += 1;
            Ok(())
        }

        fn reset(&mut self) -> std::result::Result<(), Self::Error> {
            self.0.lock().unwrap().resets += 1;
            Ok(())
        }
    }

    fn offer() -> SessionOffer {
        SessionOffer {
            video: Some(VideoConfig {
                codec: MediaCodec::H264,
                width: 1280,
                height: 720,
                frame_rate_numerator: 30,
                frame_rate_denominator: 1,
            }),
            audio: Some(AudioConfig {
                codec: MediaCodec::Opus,
                sample_rate: 48_000,
                channels: 2,
            }),
            latency_ms: 120,
        }
    }

    fn timestamp(value: i64) -> MediaTimestamp {
        MediaTimestamp::new(value, TimeBase::new(1, 90_000).unwrap())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_media_stream_reaches_renderers_and_cleans_up() {
        let server = Arc::new(
            PairingServer::bind(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("123456").unwrap(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let accepting = Arc::clone(&server);
        let counts = Arc::new(Mutex::new(Counts::default()));
        let receiver_counts = Arc::clone(&counts);
        let receiver_task = tokio::spawn(async move {
            let transport = accepting
                .accept_pairing(Duration::from_secs(5))
                .await
                .unwrap();
            let stream = transport
                .accept_media_stream(Duration::from_secs(5))
                .await
                .unwrap();
            let mut session = MediaSession::prepare(
                offer(),
                Some((
                    TrackId::new("video-0").unwrap(),
                    FakeVideo(Arc::clone(&receiver_counts)),
                )),
                Some((
                    TrackId::new("audio-0").unwrap(),
                    FakeAudio(Arc::clone(&receiver_counts)),
                )),
            )
            .unwrap();
            session.start().unwrap();
            NativeMediaReceiver::new(stream, session)
                .serve(Duration::from_secs(5))
                .await
                .unwrap()
        });

        let transport = PairingClient::connect(
            address,
            "localhost",
            &certificate,
            PairingCode::parse("123456").unwrap(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let mut sender = transport.open_media_stream().await.unwrap();
        let video = MediaPacket::Video(
            EncodedVideoFrame::new(
                TrackId::new("video-0").unwrap(),
                1,
                timestamp(0),
                None,
                true,
                vec![1, 2, 3],
            )
            .unwrap(),
        );
        let audio = MediaPacket::Audio(
            AudioPacket::new(
                TrackId::new("audio-0").unwrap(),
                1,
                timestamp(0),
                960,
                vec![4, 5],
            )
            .unwrap(),
        );
        sender
            .send_frame(&media_wire::encode(&video).unwrap(), Duration::from_secs(2))
            .await
            .unwrap();
        sender
            .send_frame(&media_wire::encode(&audio).unwrap(), Duration::from_secs(2))
            .await
            .unwrap();
        sender.finish().unwrap();

        let report = receiver_task.await.unwrap();
        assert_eq!(
            report,
            MediaStreamReport {
                frames: 2,
                video_frames: 1,
                audio_frames: 1,
                payload_bytes: 5,
            }
        );
        let counts = counts.lock().unwrap();
        assert_eq!(counts.video, 1);
        assert_eq!(counts.audio, 1);
        assert_eq!(counts.resets, 2);
        transport.close();
        server.close();
    }
}

impl<V, A> NativeMediaReceiver<V, A>
where
    V: VideoRenderer,
    A: AudioRenderer,
{
    /// Creates a receiver. The supplied media session must be started before
    /// [`serve`](Self::serve); no packets are accepted while it is preparing.
    pub fn new(stream: MediaReceiver, session: MediaSession<V, A>) -> Self {
        Self { stream, session }
    }

    /// Receives until the sender finishes the stream or a bounded error occurs.
    ///
    /// A clean stream end and an abrupt transport close are both represented by
    /// the transport's existing `ConnectionClosed` result. In either case the
    /// renderer session is reset before this method returns. Decode and render
    /// errors are returned with redacted messages and also trigger cleanup.
    pub async fn serve(mut self, budget: Duration) -> Result<MediaStreamReport> {
        if self.session.state() != SessionState::Streaming {
            let error = FrameArkError::invalid_state(
                "fanp.media_receive_state",
                "media session must be streaming before receiving frames",
            );
            let _ = self.session.reset();
            return Err(error);
        }

        let result = self.receive_frames(budget).await;
        let reset = self.session.reset();
        match (result, reset) {
            (Ok(report), Ok(())) => Ok(report),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    async fn receive_frames(&mut self, budget: Duration) -> Result<MediaStreamReport> {
        let mut report = MediaStreamReport::default();
        loop {
            let bytes = match self.stream.receive_frame(budget).await {
                Ok(bytes) => bytes,
                // MediaReceiver reports FIN and a peer disconnect through the
                // same bounded error. Both terminate the owned session safely.
                Err(TransportError::ConnectionClosed) => return Ok(report),
                Err(error) => return Err(transport_error(error)),
            };
            let packet = media_wire::decode(&bytes)?;
            self.session.render(&packet)?;
            report.frames = report.frames.saturating_add(1);
            report.payload_bytes = report
                .payload_bytes
                .saturating_add(packet.payload_len() as u64);
            match packet {
                frameark_media::MediaPacket::Video(_) => {
                    report.video_frames = report.video_frames.saturating_add(1)
                }
                frameark_media::MediaPacket::Audio(_) => {
                    report.audio_frames = report.audio_frames.saturating_add(1)
                }
            }
        }
    }
}
