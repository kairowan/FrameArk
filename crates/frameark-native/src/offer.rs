use frameark_core::{AudioConfig, Capability, FrameArkError, MediaCodec, Result, VideoConfig};
use frameark_transport::NegotiatedCapabilities;

/// A concrete SDR media proposal; no codec initialization or media bytes are carried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionOffer {
    /// Optional video track (M1 control profile supports H.264).
    pub video: Option<VideoConfig>,
    /// Optional audio track (Opus or AAC).
    pub audio: Option<AudioConfig>,
    /// Requested playout target in milliseconds, 10..=2000.
    pub latency_ms: u16,
}

impl SessionOffer {
    /// Validates the experimental wire profile independently of receiver policy.
    pub fn validate(&self) -> Result<()> {
        if self.video.is_none() && self.audio.is_none() {
            return Err(FrameArkError::invalid_argument(
                "fanp.empty_offer",
                "at least one media track is required",
            ));
        }
        if !(10..=2000).contains(&self.latency_ms) {
            return Err(FrameArkError::invalid_argument(
                "fanp.latency",
                "latency target is outside the control profile",
            ));
        }
        if let Some(video) = self.video {
            video.validate()?;
            if video.codec != MediaCodec::H264 {
                return Err(FrameArkError::unsupported(
                    "fanp.video_codec",
                    "control profile requires H.264",
                ));
            }
            if video.width > 3840
                || video.height > 2160
                || u64::from(video.frame_rate_numerator)
                    > 60 * u64::from(video.frame_rate_denominator)
            {
                return Err(FrameArkError::unsupported(
                    "fanp.video_limit",
                    "video exceeds the control profile",
                ));
            }
        }
        if let Some(audio) = self.audio {
            audio.validate()?;
            if !matches!(audio.codec, MediaCodec::Opus | MediaCodec::Aac) {
                return Err(FrameArkError::unsupported(
                    "fanp.audio_codec",
                    "control profile supports Opus or AAC",
                ));
            }
            if !(8000..=96000).contains(&audio.sample_rate) || audio.channels > 8 {
                return Err(FrameArkError::unsupported(
                    "fanp.audio_limit",
                    "audio exceeds the control profile",
                ));
            }
            if audio.codec == MediaCodec::Opus
                && ![8000, 12000, 16000, 24000, 48000].contains(&audio.sample_rate)
            {
                return Err(FrameArkError::unsupported(
                    "fanp.opus_rate",
                    "unsupported Opus sample rate",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_capabilities(
        &self,
        capabilities: &NegotiatedCapabilities,
    ) -> Result<()> {
        self.validate()?;
        if (self.video.is_some() && !capabilities.supports(Capability::Video))
            || (self.audio.is_some() && !capabilities.supports(Capability::Audio))
        {
            return Err(FrameArkError::unsupported(
                "fanp.unnegotiated_track",
                "track was not negotiated",
            ));
        }
        Ok(())
    }
}

/// Explicit receiver limits; the caller must derive these from available outputs.
/// These bounds never claim that a decoder is actually present.
#[derive(Clone, Copy, Debug)]
pub struct ReceiverPolicy {
    /// Whether an H.264 output exists.
    pub video: bool,
    /// Whether an Opus output exists.
    pub opus: bool,
    /// Whether an AAC output exists.
    pub aac: bool,
    /// Maximum receiver width.
    pub max_width: u32,
    /// Maximum receiver height.
    pub max_height: u32,
    /// Maximum whole-number frame rate.
    pub max_fps: u32,
    /// Maximum audio channel count.
    pub max_channels: u16,
}

impl ReceiverPolicy {
    /// Validates receiver bounds before accepting a remote proposal.
    pub fn validate(&self) -> Result<()> {
        if (self.video && (self.max_width == 0 || self.max_height == 0 || self.max_fps == 0))
            || ((self.opus || self.aac) && self.max_channels == 0)
        {
            return Err(FrameArkError::invalid_argument(
                "fanp.receiver_policy",
                "receiver limits must be non-zero",
            ));
        }
        Ok(())
    }

    /// Checks an exact proposal. There is no implicit codec or quality fallback.
    pub fn accepts(&self, offer: &SessionOffer) -> Result<()> {
        self.validate()?;
        offer.validate()?;
        if let Some(v) = offer.video
            && (!self.video
                || v.width > self.max_width
                || v.height > self.max_height
                || u64::from(v.frame_rate_numerator)
                    > u64::from(self.max_fps) * u64::from(v.frame_rate_denominator))
        {
            return Err(FrameArkError::unsupported(
                "fanp.receiver_video",
                "receiver cannot accept the video proposal",
            ));
        }
        if let Some(a) = offer.audio
            && (a.channels > self.max_channels
                || match a.codec {
                    MediaCodec::Opus => !self.opus,
                    MediaCodec::Aac => !self.aac,
                    _ => true,
                })
        {
            return Err(FrameArkError::unsupported(
                "fanp.receiver_audio",
                "receiver cannot accept the audio proposal",
            ));
        }
        Ok(())
    }
}
