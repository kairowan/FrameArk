//! Platform-independent encoded media contracts for FrameArk.
//!
//! This crate deliberately does not decode, render, or capture media. It
//! defines the bounded packet ownership and queue semantics shared by protocol
//! adapters, test senders, and platform renderers. A platform may copy a
//! packet into its decoder, but the Rust core remains responsible for packet
//! lifetime, timestamps, and backpressure decisions.

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};

use frameark_core::{Result, TimeBase, TrackId};

/// Maximum encoded video payload accepted by the shared contract.
pub const MAX_VIDEO_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
/// Maximum encoded audio payload accepted by the shared contract.
pub const MAX_AUDIO_PAYLOAD_BYTES: usize = 256 * 1024;

/// A monotonic presentation timestamp in a track's [`TimeBase`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MediaTimestamp {
    value: i64,
    time_base: TimeBase,
}

impl MediaTimestamp {
    /// Creates a timestamp.
    pub const fn new(value: i64, time_base: TimeBase) -> Self {
        Self { value, time_base }
    }

    /// Returns the timestamp value in ticks.
    pub const fn value(self) -> i64 {
        self.value
    }

    /// Returns the clock used by the timestamp.
    pub const fn time_base(self) -> TimeBase {
        self.time_base
    }
}

/// An encoded video access unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    track_id: TrackId,
    sequence: u64,
    presentation: MediaTimestamp,
    decode: Option<MediaTimestamp>,
    keyframe: bool,
    payload: Vec<u8>,
}

impl VideoFrame {
    /// Creates a bounded video frame and rejects an empty or oversized payload.
    pub fn new(
        track_id: TrackId,
        sequence: u64,
        presentation: MediaTimestamp,
        decode: Option<MediaTimestamp>,
        keyframe: bool,
        payload: Vec<u8>,
    ) -> Result<Self> {
        validate_payload(payload.len(), MAX_VIDEO_PAYLOAD_BYTES, "video")?;
        Ok(Self {
            track_id,
            sequence,
            presentation,
            decode,
            keyframe,
            payload,
        })
    }

    /// Returns the source track.
    pub fn track_id(&self) -> &TrackId {
        &self.track_id
    }

    /// Returns the transport sequence number.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the presentation timestamp.
    pub const fn presentation(&self) -> MediaTimestamp {
        self.presentation
    }

    /// Returns the optional decode timestamp used for reordered video.
    pub const fn decode(&self) -> Option<MediaTimestamp> {
        self.decode
    }

    /// Reports whether this access unit is a random-access point.
    pub const fn is_keyframe(&self) -> bool {
        self.keyframe
    }

    /// Returns the encoded payload without exposing mutable internal storage.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// An encoded audio access unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPacket {
    track_id: TrackId,
    sequence: u64,
    timestamp: MediaTimestamp,
    duration_ticks: u32,
    payload: Vec<u8>,
}

impl AudioPacket {
    /// Creates a bounded audio packet and rejects an empty or oversized payload.
    pub fn new(
        track_id: TrackId,
        sequence: u64,
        timestamp: MediaTimestamp,
        duration_ticks: u32,
        payload: Vec<u8>,
    ) -> Result<Self> {
        if duration_ticks == 0 {
            return Err(frameark_core::FrameArkError::invalid_argument(
                "media.zero_audio_duration",
                "audio packet duration must be non-zero",
            ));
        }
        validate_payload(payload.len(), MAX_AUDIO_PAYLOAD_BYTES, "audio")?;
        Ok(Self {
            track_id,
            sequence,
            timestamp,
            duration_ticks,
            payload,
        })
    }

    /// Returns the source track.
    pub fn track_id(&self) -> &TrackId {
        &self.track_id
    }

    /// Returns the transport sequence number.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the packet timestamp.
    pub const fn timestamp(&self) -> MediaTimestamp {
        self.timestamp
    }

    /// Returns packet duration in timestamp ticks.
    pub const fn duration_ticks(&self) -> u32 {
        self.duration_ticks
    }

    /// Returns the encoded payload without exposing mutable internal storage.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// An encoded packet crossing the protocol/platform boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MediaPacket {
    /// Video access unit.
    Video(VideoFrame),
    /// Audio access unit.
    Audio(AudioPacket),
}

impl MediaPacket {
    /// Returns the track identifier carried by the packet.
    pub fn track_id(&self) -> &TrackId {
        match self {
            Self::Video(frame) => frame.track_id(),
            Self::Audio(packet) => packet.track_id(),
        }
    }

    /// Returns the encoded payload length in bytes.
    pub fn payload_len(&self) -> usize {
        match self {
            Self::Video(frame) => frame.payload().len(),
            Self::Audio(packet) => packet.payload().len(),
        }
    }
}

fn validate_payload(length: usize, maximum: usize, media_kind: &str) -> Result<()> {
    if length == 0 {
        return Err(frameark_core::FrameArkError::invalid_argument(
            "media.empty_payload",
            format!("{media_kind} payload must not be empty"),
        ));
    }
    if length > maximum {
        return Err(frameark_core::FrameArkError::invalid_argument(
            "media.payload_too_large",
            format!("{media_kind} payload exceeds bounded media limit"),
        ));
    }
    Ok(())
}

/// Queue configuration for a single session or track group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueLimits {
    /// Maximum number of packets retained.
    pub packet_capacity: usize,
    /// Maximum encoded payload bytes retained.
    pub byte_capacity: usize,
}

impl QueueLimits {
    /// Creates limits and rejects zero capacities.
    pub fn new(packet_capacity: usize, byte_capacity: usize) -> Result<Self> {
        if packet_capacity == 0 || byte_capacity == 0 {
            return Err(frameark_core::FrameArkError::invalid_argument(
                "media.zero_queue_capacity",
                "media queue capacities must be non-zero",
            ));
        }
        Ok(Self {
            packet_capacity,
            byte_capacity,
        })
    }
}

/// Stable queue failures used to translate backpressure into protocol events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    /// The packet cannot fit within the configured queue limits.
    Full,
    /// The producer attempted to push after the queue was closed.
    Closed,
}

impl Display for QueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Full => "media queue is full",
            Self::Closed => "media queue is closed",
        })
    }
}

impl std::error::Error for QueueError {}

/// A bounded FIFO media queue with explicit backpressure.
///
/// The queue never silently drops encoded media. Producers receive
/// [`QueueError::Full`] and can then pause the transport, request a keyframe,
/// or apply a protocol-specific recovery policy.
#[derive(Debug)]
pub struct MediaQueue {
    limits: QueueLimits,
    packets: VecDeque<MediaPacket>,
    bytes: usize,
    closed: bool,
}

impl MediaQueue {
    /// Creates an empty queue with bounded packet and byte capacity.
    pub const fn new(limits: QueueLimits) -> Self {
        Self {
            limits,
            packets: VecDeque::new(),
            bytes: 0,
            closed: false,
        }
    }

    /// Adds one packet or returns an explicit backpressure/closed error.
    pub fn push(&mut self, packet: MediaPacket) -> std::result::Result<(), QueueError> {
        if self.closed {
            return Err(QueueError::Closed);
        }
        let packet_bytes = packet.payload_len();
        if packet_bytes > self.limits.byte_capacity
            || self.packets.len() >= self.limits.packet_capacity
            || self.bytes.saturating_add(packet_bytes) > self.limits.byte_capacity
        {
            return Err(QueueError::Full);
        }
        self.bytes += packet_bytes;
        self.packets.push_back(packet);
        Ok(())
    }

    /// Removes the oldest packet, if one is available.
    pub fn pop(&mut self) -> Option<MediaPacket> {
        let packet = self.packets.pop_front()?;
        self.bytes -= packet.payload_len();
        Some(packet)
    }

    /// Closes the queue. Existing packets remain available to drain.
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Reports whether no more packets can be pushed.
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns the number of queued packets.
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// Reports whether the queue contains no packets.
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    /// Returns the encoded payload bytes currently retained.
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Returns the configured limits.
    pub const fn limits(&self) -> QueueLimits {
        self.limits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> TrackId {
        TrackId::new("video-0").expect("valid track id")
    }

    fn timestamp() -> MediaTimestamp {
        MediaTimestamp::new(0, TimeBase::new(1, 90_000).expect("valid time base"))
    }

    fn frame(size: usize) -> MediaPacket {
        MediaPacket::Video(
            VideoFrame::new(track(), 0, timestamp(), None, true, vec![1; size])
                .expect("valid frame"),
        )
    }

    #[test]
    fn rejects_unbounded_payloads_and_zero_duration() {
        assert!(VideoFrame::new(track(), 0, timestamp(), None, false, Vec::new()).is_err());
        assert!(
            VideoFrame::new(
                track(),
                0,
                timestamp(),
                None,
                false,
                vec![0; MAX_VIDEO_PAYLOAD_BYTES + 1],
            )
            .is_err()
        );
        assert!(AudioPacket::new(track(), 0, timestamp(), 0, vec![1]).is_err());
    }

    #[test]
    fn queue_applies_packet_and_byte_backpressure() {
        let limits = QueueLimits::new(2, 4).expect("valid limits");
        let mut queue = MediaQueue::new(limits);
        assert_eq!(queue.push(frame(3)), Ok(()));
        assert_eq!(queue.push(frame(2)), Err(QueueError::Full));
        assert_eq!(queue.bytes(), 3);
        assert!(queue.pop().is_some());
        assert_eq!(queue.push(frame(2)), Ok(()));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn close_preserves_drain_but_rejects_new_packets() {
        let limits = QueueLimits::new(1, 4).expect("valid limits");
        let mut queue = MediaQueue::new(limits);
        queue.push(frame(2)).expect("push succeeds");
        queue.close();
        assert_eq!(queue.push(frame(1)), Err(QueueError::Closed));
        assert!(queue.pop().is_some());
        assert!(queue.is_empty());
    }
}
