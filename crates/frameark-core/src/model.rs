use std::collections::BTreeSet;

use crate::{DeviceId, FrameArkError, Result};

/// A protocol family that can expose or consume a FrameArk session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Protocol {
    /// FrameArk Native Protocol.
    Fanp,
    /// DLNA/UPnP media interoperability.
    Dlna,
    /// AirPlay and RAOP compatibility layer.
    AirPlayRaop,
    /// Cast V2 compatibility layer.
    CastCompat,
    /// Miracast/Wi-Fi Display platform adapter.
    Miracast,
}

/// A capability advertised by a device.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    /// The device can receive or produce video.
    Video,
    /// The device can receive or produce audio.
    Audio,
    /// The device supports subtitles or timed text.
    Subtitles,
    /// The device supports remote-control commands.
    RemoteControl,
}

/// A discovered FrameArk device and its advertised capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    id: DeviceId,
    name: String,
    protocols: BTreeSet<Protocol>,
    capabilities: BTreeSet<Capability>,
}

impl Device {
    /// Creates a device with no advertised protocol or media capability.
    pub fn new(id: DeviceId, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(FrameArkError::invalid_argument(
                "device.empty_name",
                "device name must not be empty",
            ));
        }
        if name.chars().count() > 128 {
            return Err(FrameArkError::invalid_argument(
                "device.name_too_long",
                "device name exceeds 128 characters",
            ));
        }
        Ok(Self {
            id,
            name,
            protocols: BTreeSet::new(),
            capabilities: BTreeSet::new(),
        })
    }

    /// Returns the stable device identifier.
    pub fn id(&self) -> &DeviceId {
        &self.id
    }

    /// Returns the user-facing device name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Adds an advertised protocol.
    pub fn add_protocol(&mut self, protocol: Protocol) {
        self.protocols.insert(protocol);
    }

    /// Adds an advertised capability.
    pub fn add_capability(&mut self, capability: Capability) {
        self.capabilities.insert(capability);
    }

    /// Checks whether the device advertises a protocol.
    pub fn supports_protocol(&self, protocol: Protocol) -> bool {
        self.protocols.contains(&protocol)
    }

    /// Checks whether the device advertises a capability.
    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Returns the advertised protocol set.
    pub fn protocols(&self) -> &BTreeSet<Protocol> {
        &self.protocols
    }

    /// Returns the advertised capability set.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.capabilities
    }
}

/// The role of a media track inside a session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TrackKind {
    /// A video track.
    Video,
    /// An audio track.
    Audio,
    /// A subtitle or timed-text track.
    Subtitles,
}

/// A codec family understood by the shared media model.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MediaCodec {
    /// H.264/AVC video.
    H264,
    /// H.265/HEVC video.
    H265,
    /// AAC audio.
    Aac,
    /// Opus audio.
    Opus,
    /// PCM audio.
    Pcm,
    /// ALAC audio.
    Alac,
    /// A codec not yet modeled by the core.
    Unknown,
}

/// A rational media clock time base.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TimeBase {
    numerator: u32,
    denominator: u32,
}

impl TimeBase {
    /// Creates a time base and rejects a zero denominator.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self> {
        if denominator == 0 {
            return Err(FrameArkError::invalid_argument(
                "media.zero_time_base_denominator",
                "time base denominator must be non-zero",
            ));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// Returns the numerator.
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the denominator.
    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// Video decoder configuration shared with platform renderers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct VideoConfig {
    /// Codec used by the encoded stream.
    pub codec: MediaCodec,
    /// Coded width in pixels.
    pub width: u32,
    /// Coded height in pixels.
    pub height: u32,
    /// Frame-rate numerator.
    pub frame_rate_numerator: u32,
    /// Frame-rate denominator.
    pub frame_rate_denominator: u32,
}

impl VideoConfig {
    /// Validates dimensions and frame-rate fields.
    pub fn validate(self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(FrameArkError::invalid_argument(
                "media.zero_video_dimension",
                "video dimensions must be non-zero",
            ));
        }
        if self.frame_rate_numerator == 0 || self.frame_rate_denominator == 0 {
            return Err(FrameArkError::invalid_argument(
                "media.invalid_frame_rate",
                "video frame rate must be non-zero",
            ));
        }
        Ok(())
    }
}

/// Audio decoder configuration shared with platform renderers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AudioConfig {
    /// Codec used by the encoded stream.
    pub codec: MediaCodec,
    /// Samples per second.
    pub sample_rate: u32,
    /// Number of interleaved channels.
    pub channels: u16,
}

impl AudioConfig {
    /// Validates sample-rate and channel fields.
    pub fn validate(self) -> Result<()> {
        if self.sample_rate == 0 || self.channels == 0 {
            return Err(FrameArkError::invalid_argument(
                "media.invalid_audio_format",
                "audio sample rate and channels must be non-zero",
            ));
        }
        Ok(())
    }
}

/// A configured media track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    id: crate::TrackId,
    kind: TrackKind,
    codec: MediaCodec,
    time_base: TimeBase,
}

impl Track {
    /// Creates a track with an explicit media clock.
    pub const fn new(
        id: crate::TrackId,
        kind: TrackKind,
        codec: MediaCodec,
        time_base: TimeBase,
    ) -> Self {
        Self {
            id,
            kind,
            codec,
            time_base,
        }
    }

    /// Returns the track identifier.
    pub fn id(&self) -> &crate::TrackId {
        &self.id
    }

    /// Returns the track role.
    pub const fn kind(&self) -> TrackKind {
        self.kind
    }

    /// Returns the codec family.
    pub const fn codec(&self) -> MediaCodec {
        self.codec
    }

    /// Returns the track time base.
    pub const fn time_base(&self) -> TimeBase {
        self.time_base
    }
}
