//! Single-session FANP control and media orchestration.
//!
//! This adapter accepts a fixed offer or explicitly negotiated video updates.
//! Updates follow media-stream FIN so configuration cannot overtake old frames.
//! It is the first complete Rust-side M1 sequence; platform renderers remain
//! supplied by the caller through [`MediaRendererFactory`].

use std::time::Duration;

use frameark_api::{AudioRenderer, VideoRenderer};
use frameark_core::{ErrorKind, FrameArkError, Result, Session, SessionState, VideoConfig};
use frameark_transport::{PairingSession, TransportError};

use crate::media_receiver::{MediaStreamReport, NativeMediaReceiver};
use crate::media_session::MediaSession;
use crate::wire::{Command, Request, Response, Status};
use crate::{Lifecycle, MAX_REQUESTS, ReceiverPolicy, SessionBackend, SessionOffer, SessionReport};

/// Maximum video-output rebuilds in one authenticated session.
pub const MAX_VIDEO_UPDATES: u32 = 256;

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
    /// Successfully acknowledged video-only changes, not reconnects.
    pub video_updates: u32,
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
    offer: Option<SessionOffer>,
    reconfigurable: bool,
    video_updates: u32,
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
            offer: None,
            reconfigurable: false,
            video_updates: 0,
        })
    }

    /// Serves one authenticated session, optionally with ordered video updates.
    pub async fn serve(mut self, budget: Duration) -> Result<NativeMediaSessionReport> {
        let mut expected_id = 1;
        self.accept_offer(expected_id, budget).await?;
        expected_id += 1;
        self.accept_start(expected_id, budget).await?;
        expected_id += 1;

        let mut total = MediaStreamReport::default();
        loop {
            let stream = self
                .transport
                .accept_media_stream(budget)
                .await
                .map_err(transport_error)?;
            let media = self.media.take().ok_or_else(state_error)?;
            let receiver = NativeMediaReceiver::new(stream, media);
            let report = if self.reconfigurable {
                let (media, report) = receiver.serve_segment(budget).await?;
                self.media = Some(media);
                report
            } else {
                receiver.serve(budget).await?
            };
            total.frames = total.frames.saturating_add(report.frames);
            total.video_frames = total.video_frames.saturating_add(report.video_frames);
            total.audio_frames = total.audio_frames.saturating_add(report.audio_frames);
            total.payload_bytes = total.payload_bytes.saturating_add(report.payload_bytes);
            let pending = self
                .transport
                .accept_control(budget)
                .await
                .map_err(transport_error)?;
            let request = Request::decode(pending.request())?;
            if request.id != expected_id || expected_id > MAX_REQUESTS {
                return Err(state_error());
            }
            expected_id += 1;
            if let Command::UpdateVideo(video) = request.command {
                let result = self.update_video(video);
                let status = outcome_status(&result);
                if result.is_err() {
                    self.cleanup();
                }
                pending
                    .respond(
                        Response {
                            id: request.id,
                            status,
                            state: self.lifecycle.session.state(),
                            offer: result.as_ref().ok().and(self.offer),
                        }
                        .encode()?,
                        budget,
                    )
                    .await
                    .map_err(transport_error)?;
                result?;
                continue;
            }
            if request.command != Command::Stop {
                return Err(state_error());
            }
            let reset = self.media.take().map_or(Ok(()), |mut media| media.reset());
            let stop = self.stop_backend();
            let result = reset.and(stop);
            self.lifecycle.close();
            pending
                .respond(
                    Response {
                        id: request.id,
                        status: outcome_status(&result),
                        state: SessionState::Closed,
                        offer: None,
                    }
                    .encode()?,
                    budget,
                )
                .await
                .map_err(transport_error)?;
            result?;
            self.lifecycle.diagnostic("fanp.media_session.stopped");
            self.transport.close();
            return Ok(NativeMediaSessionReport {
                control: self.lifecycle.report(request.id),
                media: total,
                video_updates: self.video_updates,
            });
        }
    }

    fn update_video(&mut self, video: VideoConfig) -> Result<()> {
        if !self.reconfigurable
            || self.video_updates >= MAX_VIDEO_UPDATES
            || self.lifecycle.session.state() != SessionState::Streaming
        {
            return Err(state_error());
        }
        let mut offer = self.offer.ok_or_else(state_error)?;
        offer.video = Some(video);
        offer.validate_capabilities(self.transport.negotiated_capabilities())?;
        self.policy.accepts(&offer)?;
        self.lifecycle.transition(SessionState::Reconfiguring)?;
        self.backend.reconfigure_video(video).map_err(|_| {
            FrameArkError::new(
                ErrorKind::Media,
                "fanp.media_backend_reconfigure",
                "receiver video reconfiguration failed",
            )
        })?;
        self.media
            .as_mut()
            .ok_or_else(state_error)?
            .reconfigure_video(video)?;
        self.offer = Some(offer);
        self.video_updates += 1;
        self.lifecycle.transition(SessionState::Streaming)?;
        self.lifecycle.diagnostic("fanp.video.reconfigured");
        Ok(())
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
            Command::OfferReconfigurable(offer) if request.id == expected_id => {
                self.reconfigurable = true;
                offer
            }
            _ => {
                self.cleanup();
                return Err(state_error());
            }
        };
        let result = if self.reconfigurable
            && (!self.backend.supports_video_reconfiguration() || offer.video.is_none())
        {
            Err(FrameArkError::unsupported(
                "fanp.video_reconfiguration",
                "receiver cannot negotiate video updates",
            ))
        } else {
            self.prepare(offer)
        };
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
        self.offer = Some(offer);
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

fn outcome_status(result: &Result<()>) -> Status {
    match result.as_ref().err().map(FrameArkError::kind) {
        None => Status::Ok,
        Some(ErrorKind::Unsupported) => Status::Unsupported,
        Some(ErrorKind::InvalidState) => Status::InvalidState,
        Some(ErrorKind::Media) => Status::Backend,
        _ => Status::Invalid,
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
        supports_updates: bool,
        fail_update: bool,
        video_configs: Vec<VideoConfig>,
        audio_configs: usize,
        video_times: Vec<i64>,
        audio_times: Vec<i64>,
    }

    struct FakeBackend(Arc<Mutex<Calls>>);

    impl SessionBackend for FakeBackend {
        fn supports_video_reconfiguration(&self) -> bool {
            self.0.lock().unwrap().supports_updates
        }

        fn reconfigure_video(&mut self, _video: VideoConfig) -> Result<()> {
            let mut calls = self.0.lock().unwrap();
            calls.backend.push("update");
            if calls.fail_update {
                return Err(state_error());
            }
            Ok(())
        }

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

        fn configure(&mut self, config: VideoConfig) -> std::result::Result<(), Self::Error> {
            self.0.lock().unwrap().video_configs.push(config);
            Ok(())
        }

        fn render(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
            let mut calls = self.0.lock().unwrap();
            calls.video += 1;
            calls.video_times.push(frame.timestamp);
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
            self.0.lock().unwrap().audio_configs += 1;
            Ok(())
        }

        fn render(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
            let mut calls = self.0.lock().unwrap();
            calls.audio += 1;
            calls.audio_times.push(frame.timestamp);
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

    struct Harness {
        server: Arc<PairingServer>,
        sender: NativeSender,
        task: tokio::task::JoinHandle<Result<NativeMediaSessionReport>>,
        calls: Arc<Mutex<Calls>>,
    }

    async fn harness(supports_updates: bool, budget: Duration) -> Harness {
        let server = Arc::new(
            PairingServer::bind_with_capabilities(
                "127.0.0.1:0".parse().unwrap(),
                "localhost",
                PairingCode::parse("123456").unwrap(),
                CapabilityOffer::default_capabilities(),
            )
            .unwrap(),
        );
        let calls = Arc::new(Mutex::new(Calls {
            supports_updates,
            ..Calls::default()
        }));
        let accepting = Arc::clone(&server);
        let receiver_calls = Arc::clone(&calls);
        let task = tokio::spawn(async move {
            let transport = accepting
                .accept_pairing(Duration::from_secs(5))
                .await
                .unwrap();
            NativeMediaControlReceiver::new(
                transport,
                session("updates-receiver"),
                ReceiverPolicy {
                    video: true,
                    opus: true,
                    aac: false,
                    max_width: 1920,
                    max_height: 1920,
                    max_fps: 60,
                    max_channels: 2,
                },
                FakeBackend(Arc::clone(&receiver_calls)),
                FakeFactory(receiver_calls),
            )
            .unwrap()
            .serve(budget)
            .await
        });
        let transport = PairingClient::connect(
            server.local_addr().unwrap(),
            "localhost",
            server.certificate_der(),
            PairingCode::parse("123456").unwrap(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        Harness {
            server,
            sender: NativeSender::new(transport, session("updates-sender")).unwrap(),
            task,
            calls,
        }
    }

    const BUDGET: Duration = Duration::from_secs(5);

    async fn segment(sender: &NativeSender, index: u64, keyframe: bool) {
        let mut stream = sender.open_media_stream().await.unwrap();
        let pts = timestamp(index as i64 * 3000);
        for packet in [
            MediaPacket::Video(
                EncodedVideoFrame::new(
                    TrackId::new("video-0").unwrap(),
                    index,
                    pts,
                    None,
                    keyframe,
                    vec![1],
                )
                .unwrap(),
            ),
            MediaPacket::Audio(
                AudioPacket::new(TrackId::new("audio-0").unwrap(), index, pts, 960, vec![2])
                    .unwrap(),
            ),
        ] {
            stream
                .send_frame(&media_wire::encode(&packet).unwrap(), BUDGET)
                .await
                .unwrap();
        }
        stream.finish().unwrap();
    }

    fn portrait() -> VideoConfig {
        VideoConfig {
            width: 720,
            height: 1280,
            ..offer().video.unwrap()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ordered_updates_keep_audio_timestamps_and_one_authenticated_session() {
        let mut h = harness(true, BUDGET).await;
        h.sender
            .offer_reconfigurable(offer(), BUDGET)
            .await
            .unwrap();
        h.sender.start(BUDGET).await.unwrap();
        segment(&h.sender, 0, true).await;
        for (index, video) in [portrait(), offer().video.unwrap(), portrait()]
            .into_iter()
            .enumerate()
        {
            h.sender.reconfigure_video(video, BUDGET).await.unwrap();
            assert_eq!(h.sender.state(), SessionState::Streaming);
            assert_eq!(h.calls.lock().unwrap().resets, 0);
            assert_eq!(h.calls.lock().unwrap().audio_configs, 1);
            segment(&h.sender, index as u64 + 1, true).await;
        }
        h.sender.stop(BUDGET).await.unwrap();
        let report = h.task.await.unwrap().unwrap();
        assert_eq!(report.video_updates, 3);
        assert_eq!(report.media.frames, 8);
        let calls = h.calls.lock().unwrap();
        assert_eq!(
            calls.backend,
            ["prepare", "start", "update", "update", "update", "reset"]
        );
        assert_eq!(
            calls.video_configs,
            [
                offer().video.unwrap(),
                portrait(),
                offer().video.unwrap(),
                portrait()
            ]
        );
        assert_eq!(calls.video_times, [0, 3000, 6000, 9000]);
        assert_eq!(calls.audio_times, calls.video_times);
        assert_eq!(calls.resets, 2);
        h.server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unsupported_receiver_rejects_adaptive_offer_before_preparing() {
        let mut h = harness(false, BUDGET).await;
        assert!(
            h.sender
                .offer_reconfigurable(offer(), BUDGET)
                .await
                .is_err()
        );
        assert_eq!(h.sender.state(), SessionState::Closed);
        assert_eq!(
            h.task.await.unwrap().unwrap_err().kind(),
            ErrorKind::Unsupported
        );
        assert!(h.calls.lock().unwrap().backend.is_empty());
        assert!(h.calls.lock().unwrap().video_configs.is_empty());
        h.server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unnegotiated_replayed_and_out_of_policy_updates_fail_closed() {
        for case in 0..3 {
            let mut h = harness(true, BUDGET).await;
            if case == 0 {
                h.sender.offer(offer(), BUDGET).await.unwrap();
            } else {
                h.sender
                    .offer_reconfigurable(offer(), BUDGET)
                    .await
                    .unwrap();
            }
            h.sender.start(BUDGET).await.unwrap();
            segment(&h.sender, 0, true).await;
            let video = if case == 2 {
                VideoConfig {
                    width: 3840,
                    ..portrait()
                }
            } else {
                portrait()
            };
            let request = Request {
                id: if case == 1 { 2 } else { 3 },
                command: Command::UpdateVideo(video),
            }
            .encode()
            .unwrap();
            let _ = h.sender.transport.request_control(request, BUDGET).await;
            assert!(h.task.await.unwrap().is_err());
            let calls = h.calls.lock().unwrap();
            assert_eq!(calls.backend, ["prepare", "start", "reset"]);
            assert_eq!(calls.resets, 2);
            h.server.close();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backend_failure_and_missing_post_update_keyframe_release_both_tracks() {
        for backend_failure in [true, false] {
            let mut h = harness(true, BUDGET).await;
            h.sender
                .offer_reconfigurable(offer(), BUDGET)
                .await
                .unwrap();
            h.sender.start(BUDGET).await.unwrap();
            segment(&h.sender, 0, true).await;
            h.calls.lock().unwrap().fail_update = backend_failure;
            let update = h.sender.reconfigure_video(portrait(), BUDGET).await;
            if backend_failure {
                assert!(update.is_err());
            } else {
                update.unwrap();
                segment(&h.sender, 1, false).await;
            }
            let error = h.task.await.unwrap().unwrap_err();
            assert_eq!(
                error.code(),
                if backend_failure {
                    "fanp.media_backend_reconfigure"
                } else {
                    "fanp.video_keyframe"
                }
            );
            assert_eq!(h.calls.lock().unwrap().video, 1);
            assert_eq!(h.calls.lock().unwrap().resets, 2);
            h.server.close();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_limit_is_enforced_without_rebuilding_outputs_again() {
        let mut h = harness(true, BUDGET).await;
        h.sender
            .offer_reconfigurable(offer(), BUDGET)
            .await
            .unwrap();
        h.sender.start(BUDGET).await.unwrap();
        for index in 0..=MAX_VIDEO_UPDATES {
            // Even empty media segments cannot evade the output-rebuild cap.
            h.sender
                .open_media_stream()
                .await
                .unwrap()
                .finish()
                .unwrap();
            let result = h.sender.reconfigure_video(portrait(), BUDGET).await;
            assert_eq!(result.is_ok(), index < MAX_VIDEO_UPDATES);
        }
        assert!(h.task.await.unwrap().is_err());
        assert_eq!(
            h.calls.lock().unwrap().video_configs.len(),
            MAX_VIDEO_UPDATES as usize + 1
        );
        assert_eq!(h.calls.lock().unwrap().resets, 2);
        h.server.close();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timeout_and_task_cancellation_release_retained_renderers() {
        for cancel in [false, true] {
            let mut h = harness(true, Duration::from_secs(1)).await;
            h.sender
                .offer_reconfigurable(offer(), BUDGET)
                .await
                .unwrap();
            h.sender.start(BUDGET).await.unwrap();
            segment(&h.sender, 0, true).await;
            h.sender
                .reconfigure_video(portrait(), BUDGET)
                .await
                .unwrap();
            if cancel {
                h.task.abort();
                assert!(h.task.await.unwrap_err().is_cancelled());
            } else {
                assert_eq!(
                    h.task.await.unwrap().unwrap_err().kind(),
                    ErrorKind::Timeout
                );
            }
            assert_eq!(h.calls.lock().unwrap().resets, 2);
            assert_eq!(h.calls.lock().unwrap().backend.last(), Some(&"reset"));
            h.server.close();
        }
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
