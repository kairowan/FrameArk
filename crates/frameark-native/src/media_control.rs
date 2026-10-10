//! Single-session FANP control and media orchestration.
//!
//! This adapter is intentionally narrow: it accepts one Offer, starts the
//! platform backend, receives one bounded media stream, and waits for Stop.
//! It is the first complete Rust-side M1 sequence; platform renderers remain
//! supplied by the caller through [`MediaRendererFactory`].

use std::time::Duration;

use frameark_api::{AudioRenderer, VideoRenderer};
use frameark_core::{ErrorKind, FrameArkError, Result, Session, SessionState};
use frameark_transport::{PairingSession, TransportError};

use crate::media_receiver::{MediaStreamReport, NativeMediaReceiver};
use crate::media_session::MediaSession;
use crate::wire::{Command, Request, Response, Status};
use crate::{Lifecycle, ReceiverPolicy, SessionBackend, SessionOffer, SessionReport};

/// Creates platform renderer adapters after the negotiated Offer is validated.
pub trait MediaRendererFactory {
    /// Video renderer selected for the current offer.
    type Video: VideoRenderer;
    /// Audio renderer selected for the current offer.
    type Audio: AudioRenderer;

    /// Configures platform outputs for this exact offer.
    fn prepare(&mut self, offer: SessionOffer) -> Result<MediaSession<Self::Video, Self::Audio>>;
}

/// Bounded report for one complete Offer → Start → media → Stop sequence.
#[derive(Clone, Debug)]
pub struct NativeMediaSessionReport {
    /// Control-plane lifecycle summary.
    pub control: SessionReport,
    /// Media-plane frame and byte counters.
    pub media: MediaStreamReport,
}

fn transport_error(error: TransportError) -> FrameArkError {
    let kind = if error == TransportError::Timeout {
        ErrorKind::Timeout
    } else {
        ErrorKind::Transport
    };
    FrameArkError::new(
        kind,
        "fanp.media_control_transport",
        "media control transport failed",
    )
}

fn state_error() -> FrameArkError {
    FrameArkError::invalid_state(
        "fanp.media_control_state",
        "media control command is not valid in the current state",
    )
}

/// Receiver-side owner for one complete Native control/media session.
pub struct NativeMediaControlReceiver<B, F>
where
    B: SessionBackend,
    F: MediaRendererFactory,
{
    transport: PairingSession,
    lifecycle: Lifecycle,
    policy: ReceiverPolicy,
    backend: B,
    factory: F,
    backend_dirty: bool,
    media: Option<MediaSession<F::Video, F::Audio>>,
}

impl<B, F> NativeMediaControlReceiver<B, F>
where
    B: SessionBackend,
    F: MediaRendererFactory,
{
    /// Attaches a fresh shared core session, policy, backend, and renderer factory.
    pub fn new(
        transport: PairingSession,
        session: Session,
        policy: ReceiverPolicy,
        backend: B,
        factory: F,
    ) -> Result<Self> {
        policy.validate()?;
        let lifecycle = Lifecycle::new(&transport, session)?;
        Ok(Self {
            transport,
            lifecycle,
            policy,
            backend,
            factory,
            backend_dirty: false,
            media: None,
        })
    }

    /// Serves one Offer, Start, media stream, and Stop sequence.
    pub async fn serve(mut self, budget: Duration) -> Result<NativeMediaSessionReport> {
        let mut expected_id = 1;
        self.accept_offer(expected_id, budget).await?;
        expected_id += 1;
        self.accept_start(expected_id, budget).await?;
        expected_id += 1;

        let stream = self
            .transport
            .accept_media_stream(budget)
            .await
            .map_err(transport_error)?;
        let media = self.media.take().ok_or_else(state_error)?;
        let media_report = match NativeMediaReceiver::new(stream, media).serve(budget).await {
            Ok(report) => report,
            Err(error) => {
                self.cleanup();
                return Err(error);
            }
        };

        let pending = self
            .transport
            .accept_control(budget)
            .await
            .map_err(transport_error)?;
        let request = Request::decode(pending.request())?;
        if request.id != expected_id || request.command != Command::Stop {
            self.cleanup();
            return Err(state_error());
        }
        let stop_result = self.stop_backend();
        let status = if stop_result.is_ok() {
            Status::Ok
        } else {
            Status::Backend
        };
        self.lifecycle.close();
        pending
            .respond(
                Response {
                    id: request.id,
                    status,
                    state: self.lifecycle.session.state(),
                    offer: None,
                }
                .encode()?,
                budget,
            )
            .await
            .map_err(transport_error)?;
        stop_result?;
        self.lifecycle.diagnostic("fanp.media_session.stopped");
        // The sender waits for the receiver to close after acknowledging Stop.
        // Drop/explicit close provides that terminal signal; waiting here would
        // deadlock because both peers otherwise remain alive.
        self.transport.close();
        Ok(NativeMediaSessionReport {
            control: self.lifecycle.report(request.id),
            media: media_report,
        })
    }

    async fn accept_offer(&mut self, expected_id: u32, budget: Duration) -> Result<SessionOffer> {
        let pending = self
            .transport
            .accept_control(budget)
            .await
            .map_err(transport_error)?;
        let request = Request::decode(pending.request())?;
        let offer = match request.command {
            Command::Offer(offer) if request.id == expected_id => offer,
            _ => {
                self.cleanup();
                return Err(state_error());
            }
        };
        let result = self.prepare(offer);
        let status = if result.is_ok() {
            Status::Ok
        } else {
            match result.as_ref().err().map(FrameArkError::kind) {
                Some(ErrorKind::Unsupported) => Status::Unsupported,
                Some(ErrorKind::InvalidState) => Status::InvalidState,
                Some(ErrorKind::Media) => Status::Backend,
                _ => Status::Invalid,
            }
        };
        let selected_offer = result.as_ref().ok().map(|_| offer);
        pending
            .respond(
                Response {
                    id: request.id,
                    status,
                    state: self.lifecycle.session.state(),
                    offer: selected_offer,
                }
                .encode()?,
                budget,
            )
            .await
            .map_err(transport_error)?;
        result.map(|_| offer)
    }

    async fn accept_start(&mut self, expected_id: u32, budget: Duration) -> Result<()> {
        let pending = self
            .transport
            .accept_control(budget)
            .await
            .map_err(transport_error)?;
        let request = Request::decode(pending.request())?;
        if request.id != expected_id || request.command != Command::Start {
            self.cleanup();
            return Err(state_error());
        }
        let result = self.start_backend();
        if result.is_ok() {
            if let Some(media) = self.media.as_mut() {
                media.start()?;
            } else {
                self.cleanup();
                return Err(state_error());
            }
            self.lifecycle.transition(SessionState::Streaming)?;
            self.lifecycle.diagnostic("fanp.media_session.started");
        }
        let status = if result.is_ok() {
            Status::Ok
        } else {
            Status::Backend
        };
        pending
            .respond(
                Response {
                    id: request.id,
                    status,
                    state: self.lifecycle.session.state(),
                    offer: None,
                }
                .encode()?,
                budget,
            )
            .await
            .map_err(transport_error)?;
        result
    }

    fn prepare(&mut self, offer: SessionOffer) -> Result<()> {
        if self.lifecycle.session.state() != SessionState::Negotiating {
            return Err(state_error());
        }
        offer.validate_capabilities(self.transport.negotiated_capabilities())?;
        self.policy.accepts(&offer)?;
        self.backend_dirty = true;
        self.backend.prepare(offer).map_err(|_| {
            FrameArkError::new(
                ErrorKind::Media,
                "fanp.media_backend_prepare",
                "receiver backend preparation failed",
            )
        })?;
        let media = match self.factory.prepare(offer) {
            Ok(media) => media,
            Err(error) => {
                let _ = self.backend.reset();
                self.backend_dirty = false;
                return Err(error);
            }
        };
        self.media = Some(media);
        self.lifecycle.transition(SessionState::Preparing)?;
        self.lifecycle.diagnostic("fanp.media_session.prepared");
        Ok(())
    }

    fn start_backend(&mut self) -> Result<()> {
        if self.lifecycle.session.state() != SessionState::Preparing {
            return Err(state_error());
        }
        self.backend.start().map_err(|_| {
            FrameArkError::new(
                ErrorKind::Media,
                "fanp.media_backend_start",
                "receiver backend start failed",
            )
        })
    }

    fn stop_backend(&mut self) -> Result<()> {
        if self.lifecycle.session.state() == SessionState::Streaming {
            self.lifecycle.transition(SessionState::Draining)?;
        }
        if self.backend_dirty {
            self.backend.reset().map_err(|_| {
                FrameArkError::new(
                    ErrorKind::Media,
                    "fanp.media_backend_reset",
                    "receiver backend reset failed",
                )
            })?;
            self.backend_dirty = false;
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        if self.backend_dirty {
            let _ = self.backend.reset();
            self.backend_dirty = false;
        }
        if let Some(mut media) = self.media.take() {
            let _ = media.reset();
        }
        self.lifecycle.close();
    }
}

impl<B, F> Drop for NativeMediaControlReceiver<B, F>
where
    B: SessionBackend,
    F: MediaRendererFactory,
{
    fn drop(&mut self) {
        self.cleanup();
        self.transport.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeSender, media_wire};
    use frameark_api::{AudioFrame, VideoFrame};
    use frameark_core::{
        AudioConfig, DeviceId, MediaCodec, SessionId, TimeBase, TrackId, VideoConfig,
    };
    use frameark_media::{
        AudioPacket, MediaPacket, MediaTimestamp, VideoFrame as EncodedVideoFrame,
    };
    use frameark_transport::{CapabilityOffer, PairingClient, PairingCode, PairingServer};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Calls {
        backend: Vec<&'static str>,
        video: usize,
        audio: usize,
        resets: usize,
    }

    struct FakeBackend(Arc<Mutex<Calls>>);

    impl SessionBackend for FakeBackend {
        fn prepare(&mut self, _offer: SessionOffer) -> Result<()> {
            self.0.lock().unwrap().backend.push("prepare");
            Ok(())
        }

        fn start(&mut self) -> Result<()> {
            self.0.lock().unwrap().backend.push("start");
            Ok(())
        }

        fn reset(&mut self) -> Result<()> {
            self.0.lock().unwrap().backend.push("reset");
            Ok(())
        }
    }

    struct FakeVideo(Arc<Mutex<Calls>>);

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

    struct FakeAudio(Arc<Mutex<Calls>>);

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

    struct FakeFactory(Arc<Mutex<Calls>>);

    impl MediaRendererFactory for FakeFactory {
        type Video = FakeVideo;
        type Audio = FakeAudio;

        fn prepare(
            &mut self,
            offer: SessionOffer,
        ) -> Result<MediaSession<Self::Video, Self::Audio>> {
            MediaSession::prepare(
                offer,
                Some((
                    TrackId::new("video-0").unwrap(),
                    FakeVideo(Arc::clone(&self.0)),
                )),
                Some((
                    TrackId::new("audio-0").unwrap(),
                    FakeAudio(Arc::clone(&self.0)),
                )),
            )
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

    fn session(id: &str) -> Session {
        Session::new(
            SessionId::try_from(id).unwrap(),
            DeviceId::try_from("receiver-1").unwrap(),
        )
    }

    fn timestamp(value: i64) -> MediaTimestamp {
        MediaTimestamp::new(value, TimeBase::new(1, 90_000).unwrap())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn control_and_media_complete_one_native_session() {
        let server = Arc::new(
            PairingServer::bind_with_capabilities(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                "localhost",
                PairingCode::parse("123456").unwrap(),
                CapabilityOffer::default_capabilities(),
            )
            .unwrap(),
        );
        let address = server.local_addr().unwrap();
        let certificate = server.certificate_der().to_vec();
        let calls = Arc::new(Mutex::new(Calls::default()));
        let accepting = Arc::clone(&server);
        let receiver_calls = Arc::clone(&calls);
        let receiver_task = tokio::spawn(async move {
            let transport = accepting
                .accept_pairing(Duration::from_secs(5))
                .await
                .unwrap();
            let policy = ReceiverPolicy {
                video: true,
                opus: true,
                aac: false,
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
                max_channels: 2,
            };
            NativeMediaControlReceiver::new(
                transport,
                session("receiver-session"),
                policy,
                FakeBackend(Arc::clone(&receiver_calls)),
                FakeFactory(receiver_calls),
            )
            .unwrap()
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
        let mut sender = NativeSender::new(transport, session("sender-session")).unwrap();
        sender.offer(offer(), Duration::from_secs(2)).await.unwrap();
        sender.start(Duration::from_secs(2)).await.unwrap();
        let mut stream = sender.transport.open_media_stream().await.unwrap();
        let video = MediaPacket::Video(
            EncodedVideoFrame::new(
                TrackId::new("video-0").unwrap(),
                0,
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
                0,
                timestamp(0),
                960,
                vec![4, 5],
            )
            .unwrap(),
        );
        stream
            .send_frame(&media_wire::encode(&video).unwrap(), Duration::from_secs(2))
            .await
            .unwrap();
        stream
            .send_frame(&media_wire::encode(&audio).unwrap(), Duration::from_secs(2))
            .await
            .unwrap();
        stream.finish().unwrap();
        let report = sender.stop(Duration::from_secs(2)).await.unwrap();
        assert_eq!(report.state, SessionState::Closed);
        let receiver_report = receiver_task.await.unwrap();
        assert_eq!(receiver_report.media.frames, 2);
        assert_eq!(receiver_report.media.video_frames, 1);
        assert_eq!(receiver_report.media.audio_frames, 1);
        assert_eq!(
            &calls.lock().unwrap().backend,
            &["prepare", "start", "reset"]
        );
        assert_eq!(calls.lock().unwrap().video, 1);
        assert_eq!(calls.lock().unwrap().audio, 1);
        assert_eq!(calls.lock().unwrap().resets, 2);
        server.close();
    }
}
