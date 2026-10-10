//! Native media-session adapter for platform-owned decoders and outputs.
//!
//! Protocol code supplies a validated [`SessionOffer`] and decoded
//! `frameark-media` packets. This module owns track matching, renderer
//! configuration, lifecycle cleanup, and redacted platform-error mapping. It
//! never touches `Surface`, `MediaCodec`, `AudioTrack`, or a UI object.

use frameark_api::{AudioFrame, AudioRenderer, VideoFrame, VideoRenderer};
use frameark_core::{ErrorKind, FrameArkError, Result, SessionState, TrackId};
use frameark_media::MediaPacket;

use crate::SessionOffer;

fn renderer_error(code: &'static str) -> FrameArkError {
    FrameArkError::new(
        ErrorKind::Media,
        code,
        "platform renderer rejected the media operation",
    )
}

fn missing_track(kind: &'static str) -> FrameArkError {
    FrameArkError::unsupported(
        "fanp.renderer_missing",
        format!("no {kind} renderer is attached for the negotiated track"),
    )
}

fn track_mismatch(kind: &'static str) -> FrameArkError {
    FrameArkError::invalid_argument(
        "fanp.track_mismatch",
        format!("{kind} packet does not belong to the configured track"),
    )
}

struct VideoTrack<V> {
    id: TrackId,
    renderer: V,
}

struct AudioTrack<A> {
    id: TrackId,
    renderer: A,
}

/// A configured Native media session backed by platform renderer adapters.
pub struct MediaSession<V: VideoRenderer, A: AudioRenderer> {
    state: SessionState,
    video: Option<VideoTrack<V>>,
    audio: Option<AudioTrack<A>>,
}

impl<V, A> MediaSession<V, A>
where
    V: VideoRenderer,
    A: AudioRenderer,
{
    /// Configures all negotiated renderers before any packet is accepted.
    ///
    /// A failed audio configuration resets an already-configured video
    /// renderer before returning, so callers never retain a half-prepared
    /// session. Track identifiers are explicit because protocol offers may
    /// contain more than one track in a future version.
    pub fn prepare(
        offer: SessionOffer,
        video: Option<(TrackId, V)>,
        audio: Option<(TrackId, A)>,
    ) -> Result<Self> {
        offer.validate()?;
        if offer.video.is_some() != video.is_some() {
            return Err(FrameArkError::invalid_argument(
                "fanp.video_renderer_shape",
                "video renderer presence does not match the offer",
            ));
        }
        if offer.audio.is_some() != audio.is_some() {
            return Err(FrameArkError::invalid_argument(
                "fanp.audio_renderer_shape",
                "audio renderer presence does not match the offer",
            ));
        }

        let video = if let Some((id, mut renderer)) = video {
            if renderer
                .configure(offer.video.expect("validated video offer"))
                .is_err()
            {
                return Err(renderer_error("fanp.video_configure"));
            }
            Some(VideoTrack { id, renderer })
        } else {
            None
        };

        let audio = if let Some((id, mut renderer)) = audio {
            if renderer
                .configure(offer.audio.expect("validated audio offer"))
                .is_err()
            {
                if let Some(mut video) = video {
                    let _ = video.renderer.reset();
                }
                return Err(renderer_error("fanp.audio_configure"));
            }
            Some(AudioTrack { id, renderer })
        } else {
            None
        };

        Ok(Self {
            state: SessionState::Preparing,
            video,
            audio,
        })
    }

    /// Returns the media lifecycle state owned by this adapter.
    pub const fn state(&self) -> SessionState {
        self.state
    }

    /// Marks the renderers active after the Native control plane starts.
    pub fn start(&mut self) -> Result<()> {
        if self.state != SessionState::Preparing {
            return Err(FrameArkError::invalid_state(
                "fanp.media_start",
                "media renderers are not prepared",
            ));
        }
        self.state = SessionState::Streaming;
        Ok(())
    }

    /// Routes one validated packet to the matching platform renderer.
    pub fn render(&mut self, packet: &MediaPacket) -> Result<()> {
        if self.state != SessionState::Streaming {
            return Err(FrameArkError::invalid_state(
                "fanp.media_render",
                "media session is not streaming",
            ));
        }
        match packet {
            MediaPacket::Video(frame) => {
                let video = self.video.as_mut().ok_or_else(|| missing_track("video"))?;
                if video.id != *frame.track_id() {
                    return Err(track_mismatch("video"));
                }
                video
                    .renderer
                    .render(VideoFrame {
                        track_id: frame.track_id(),
                        data: frame.payload(),
                        timestamp: frame.presentation().value(),
                        keyframe: frame.is_keyframe(),
                    })
                    .map_err(|_| renderer_error("fanp.video_render"))
            }
            MediaPacket::Audio(packet) => {
                let audio = self.audio.as_mut().ok_or_else(|| missing_track("audio"))?;
                if audio.id != *packet.track_id() {
                    return Err(track_mismatch("audio"));
                }
                audio
                    .renderer
                    .render(AudioFrame {
                        track_id: packet.track_id(),
                        data: packet.payload(),
                        timestamp: packet.timestamp().value(),
                    })
                    .map_err(|_| renderer_error("fanp.audio_render"))
            }
        }
    }

    /// Releases both platform outputs. Repeated calls are safe.
    pub fn reset(&mut self) -> Result<()> {
        if self.state == SessionState::Closed {
            return Ok(());
        }
        let mut result = Ok(());
        if let Some(video) = self.video.as_mut()
            && video.renderer.reset().is_err()
        {
            result = Err(renderer_error("fanp.video_reset"));
        }
        if let Some(audio) = self.audio.as_mut()
            && audio.renderer.reset().is_err()
            && result.is_ok()
        {
            result = Err(renderer_error("fanp.audio_reset"));
        }
        self.state = SessionState::Closed;
        result
    }
}

impl<V, A> Drop for MediaSession<V, A>
where
    V: VideoRenderer,
    A: AudioRenderer,
{
    fn drop(&mut self) {
        let _ = self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frameark_core::{AudioConfig, MediaCodec, TimeBase, VideoConfig};
    use frameark_media::{AudioPacket, MediaTimestamp, VideoFrame as EncodedVideoFrame};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct VideoState {
        configured: Option<VideoConfig>,
        rendered: usize,
        resets: usize,
    }

    struct FakeVideo(Rc<RefCell<VideoState>>);

    impl VideoRenderer for FakeVideo {
        type Error = ();

        fn configure(&mut self, config: VideoConfig) -> std::result::Result<(), Self::Error> {
            self.0.borrow_mut().configured = Some(config);
            Ok(())
        }

        fn render(
            &mut self,
            _frame: frameark_api::VideoFrame<'_>,
        ) -> std::result::Result<(), Self::Error> {
            self.0.borrow_mut().rendered += 1;
            Ok(())
        }

        fn reset(&mut self) -> std::result::Result<(), Self::Error> {
            self.0.borrow_mut().resets += 1;
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct AudioState {
        configured: Option<AudioConfig>,
        rendered: usize,
        resets: usize,
        fail_configure: bool,
    }

    struct FakeAudio(Rc<RefCell<AudioState>>);

    impl AudioRenderer for FakeAudio {
        type Error = ();

        fn configure(&mut self, config: AudioConfig) -> std::result::Result<(), Self::Error> {
            if self.0.borrow().fail_configure {
                return Err(());
            }
            self.0.borrow_mut().configured = Some(config);
            Ok(())
        }

        fn render(&mut self, _frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
            self.0.borrow_mut().rendered += 1;
            Ok(())
        }

        fn reset(&mut self) -> std::result::Result<(), Self::Error> {
            self.0.borrow_mut().resets += 1;
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

    fn timestamp() -> MediaTimestamp {
        MediaTimestamp::new(0, TimeBase::new(1, 90_000).unwrap())
    }

    #[test]
    fn configures_routes_and_resets_both_tracks() {
        let video_state = Rc::new(RefCell::new(VideoState::default()));
        let audio_state = Rc::new(RefCell::new(AudioState::default()));
        let mut session = MediaSession::prepare(
            offer(),
            Some((
                TrackId::new("video-0").unwrap(),
                FakeVideo(video_state.clone()),
            )),
            Some((
                TrackId::new("audio-0").unwrap(),
                FakeAudio(audio_state.clone()),
            )),
        )
        .unwrap();
        assert_eq!(session.state(), SessionState::Preparing);
        session.start().unwrap();
        session
            .render(&MediaPacket::Video(
                EncodedVideoFrame::new(
                    TrackId::new("video-0").unwrap(),
                    1,
                    timestamp(),
                    None,
                    true,
                    vec![1],
                )
                .unwrap(),
            ))
            .unwrap();
        session
            .render(&MediaPacket::Audio(
                AudioPacket::new(
                    TrackId::new("audio-0").unwrap(),
                    1,
                    timestamp(),
                    960,
                    vec![2],
                )
                .unwrap(),
            ))
            .unwrap();
        session.reset().unwrap();
        assert_eq!(video_state.borrow().configured, offer().video);
        assert_eq!(audio_state.borrow().configured, offer().audio);
        assert_eq!(video_state.borrow().rendered, 1);
        assert_eq!(audio_state.borrow().rendered, 1);
        assert_eq!(video_state.borrow().resets, 1);
        assert_eq!(audio_state.borrow().resets, 1);
    }

    #[test]
    fn track_mismatch_is_rejected_without_rendering() {
        let video_state = Rc::new(RefCell::new(VideoState::default()));
        let mut session = MediaSession::prepare(
            offer(),
            Some((
                TrackId::new("video-0").unwrap(),
                FakeVideo(video_state.clone()),
            )),
            Some((
                TrackId::new("audio-0").unwrap(),
                FakeAudio(Rc::new(RefCell::new(AudioState::default()))),
            )),
        )
        .unwrap();
        session.start().unwrap();
        let packet = MediaPacket::Video(
            EncodedVideoFrame::new(
                TrackId::new("other-video").unwrap(),
                1,
                timestamp(),
                None,
                false,
                vec![1],
            )
            .unwrap(),
        );
        assert_eq!(
            session.render(&packet).unwrap_err().code(),
            "fanp.track_mismatch"
        );
        assert_eq!(video_state.borrow().rendered, 0);
    }

    #[test]
    fn failed_audio_prepare_resets_video() {
        let video_state = Rc::new(RefCell::new(VideoState::default()));
        let audio_state = Rc::new(RefCell::new(AudioState {
            fail_configure: true,
            ..AudioState::default()
        }));
        assert!(
            MediaSession::prepare(
                offer(),
                Some((
                    TrackId::new("video-0").unwrap(),
                    FakeVideo(video_state.clone())
                )),
                Some((TrackId::new("audio-0").unwrap(), FakeAudio(audio_state))),
            )
            .is_err()
        );
        assert_eq!(video_state.borrow().resets, 1);
    }
}
