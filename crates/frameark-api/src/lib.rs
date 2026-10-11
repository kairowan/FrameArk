//! Stable platform-facing interfaces for the FrameArk core.
//!
//! These traits describe ownership across the Rust/platform boundary. Android,
//! desktop, and future web adapters implement them with their native codec,
//! audio, clock, and rendering APIs.

use frameark_core::{AudioConfig, Result, TrackId, VideoConfig};

/// An encoded video sample handed to a platform decoder or renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoFrame<'a> {
    /// Track receiving this sample.
    pub track_id: &'a TrackId,
    /// Encoded frame bytes. Ownership remains with the caller for the call.
    pub data: &'a [u8],
    /// Presentation timestamp expressed in the track time base.
    pub timestamp: i64,
    /// Whether the sample is a random-access/key frame.
    pub keyframe: bool,
}

/// An encoded audio sample handed to a platform decoder or renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFrame<'a> {
    /// Track receiving this sample.
    pub track_id: &'a TrackId,
    /// Encoded frame bytes. Ownership remains with the caller for the call.
    pub data: &'a [u8],
    /// Presentation timestamp expressed in the track time base.
    pub timestamp: i64,
}

/// A platform-owned video rendering surface and decoder adapter.
pub trait VideoRenderer {
    /// Platform-specific error type.
    type Error;

    /// Applies a new decoder configuration.
    fn configure(&mut self, config: VideoConfig) -> std::result::Result<(), Self::Error>;

    /// Queues one encoded frame for decoding or rendering.
    fn render(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error>;

    /// Releases decoder state and any surface ownership.
    fn reset(&mut self) -> std::result::Result<(), Self::Error>;
}

/// A platform-owned audio output and decoder adapter.
pub trait AudioRenderer {
    /// Platform-specific error type.
    type Error;

    /// Applies a new audio decoder configuration.
    fn configure(&mut self, config: AudioConfig) -> std::result::Result<(), Self::Error>;

    /// Queues one encoded audio frame.
    fn render(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error>;

    /// Releases decoder and audio-output state.
    fn reset(&mut self) -> std::result::Result<(), Self::Error>;
}

/// A platform-facing sink for encoded access units received from FANP.
///
/// This boundary is useful when the platform owns decoding asynchronously,
/// such as Android's JNI queue. Rust still owns protocol framing and session
/// state; the implementation only copies or queues the bounded sample for the
/// platform codec pipeline.
pub trait EncodedMediaSink {
    /// Platform-specific queueing error type.
    type Error;

    /// Accepts one encoded video sample.
    fn push_video(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error>;

    /// Accepts one encoded audio sample.
    fn push_audio(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error>;
}

/// A monotonic clock supplied by a platform adapter.
pub trait MonotonicClock {
    /// Returns elapsed microseconds from a monotonic, non-wall-clock source.
    fn now_micros(&self) -> u64;
}

/// Platform codec and output capabilities discovered during startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformCapabilities {
    /// Whether hardware H.264 decoding is available.
    pub hardware_h264: bool,
    /// Whether hardware H.265 decoding is available.
    pub hardware_h265: bool,
    /// Whether an audio output can be opened.
    pub audio_output: bool,
}

impl PlatformCapabilities {
    /// Validates that at least one media output exists.
    pub fn validate(self) -> Result<()> {
        if !self.hardware_h264 && !self.hardware_h265 && !self.audio_output {
            return Err(frameark_core::FrameArkError::unsupported(
                "platform.no_media_output",
                "platform reports no usable media output",
            ));
        }
        Ok(())
    }
}
