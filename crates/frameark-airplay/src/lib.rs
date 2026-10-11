//! Bounded AirPlay/RAOP protocol contracts.
//!
//! This M4/M5 foundation parses RTSP control messages and unencrypted RTP
//! audio/video packets. It intentionally does not implement Apple pairing, FairPlay,
//! AES-CTR audio decryption, ALAC/AAC decoding, or device interoperability.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

/// Maximum RTSP message size accepted before parsing.
pub const MAX_RTSP_BYTES: usize = 64 * 1024;
/// Maximum RTSP body size accepted before parsing.
pub const MAX_RTSP_BODY_BYTES: usize = 32 * 1024;
/// Maximum RTP payload size accepted by the bounded audio contract.
pub const MAX_RTP_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum datagram buffer used by the RTP socket adapter.
pub const MAX_RTP_PACKET_BYTES: usize = 12 + MAX_RTP_PAYLOAD_BYTES;
/// Maximum packets retained by one RAOP RTP jitter buffer.
pub const MAX_RTP_JITTER_PACKETS: usize = 128;
/// Maximum SDP body accepted by ANNOUNCE.
pub const MAX_SDP_BYTES: usize = 16 * 1024;
/// Maximum SDP attribute lines accepted by one announcement.
pub const MAX_SDP_LINES: usize = 64;
/// Maximum RAOP session identifier length.
pub const MAX_SESSION_ID_BYTES: usize = 128;
/// Maximum XML Property List document size.
pub const MAX_PLIST_BYTES: usize = 64 * 1024;
/// Maximum nested Property List container depth.
pub const MAX_PLIST_DEPTH: usize = 8;
/// Maximum dictionary/array entries in one Property List document.
pub const MAX_PLIST_ENTRIES: usize = 64;
/// Maximum scalar text or binary data field size.
pub const MAX_PLIST_FIELD_BYTES: usize = 16 * 1024;
/// Maximum objects in one binary Property List object table.
pub const MAX_BINARY_PLIST_OBJECTS: usize = MAX_PLIST_ENTRIES * 2 + 1;
/// Maximum encoded H.264 access unit accepted by the mirror contract.
pub const MAX_MIRROR_ACCESS_UNIT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum H.264 NAL units in one mirror access unit.
pub const MAX_MIRROR_NAL_UNITS: usize = 256;
/// Maximum bytes retained while assembling one RTP mirror video access unit.
pub const MAX_MIRROR_VIDEO_ASSEMBLY_BYTES: usize = MAX_MIRROR_ACCESS_UNIT_BYTES;
/// Maximum mirror dimension accepted before platform allocation.
pub const MAX_MIRROR_DIMENSION: u16 = 8192;
/// Maximum encoded mirror audio access-unit payload.
pub const MAX_MIRROR_AUDIO_ACCESS_UNIT_BYTES: usize = 256 * 1024;
/// Maximum audio samples represented by one mirror access unit.
pub const MAX_MIRROR_AUDIO_DURATION_SAMPLES: u32 = 192_000;
/// Video clock rate used by the mirror timing contract.
pub const MIRROR_VIDEO_CLOCK_HZ: u32 = 90_000;
/// Maximum mirror RTSP session identifier length.
pub const MAX_MIRROR_SESSION_ID_BYTES: usize = 128;

/// Errors raised by bounded RTSP/RTP parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AirplayError {
    /// Input exceeded a protocol bound.
    TooLarge(&'static str),
    /// Input violated the protocol grammar.
    Invalid(&'static str),
    /// A caller supplied an invalid session field.
    InvalidField(&'static str),
    /// The operating system rejected a bounded RTP socket operation.
    Io(ErrorKind),
}

impl Display for AirplayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(field) => write!(formatter, "AirPlay {field} exceeds its bound"),
            Self::Invalid(field) => write!(formatter, "invalid AirPlay {field}"),
            Self::InvalidField(field) => write!(formatter, "invalid AirPlay field {field}"),
            Self::Io(kind) => write!(formatter, "AirPlay RTP socket operation failed: {kind}"),
        }
    }
}

/// A bounded UDP adapter for the already-negotiated unencrypted RTP profile.
///
/// The adapter owns only the datagram socket and packet parsing. It does not
/// select codecs, decrypt payloads, join multicast groups, or advance a
/// jitter buffer. Callers should apply the RTSP-negotiated peer, SSRC, payload
/// type, and loss policy before handing packets to a media pipeline.
#[derive(Debug)]
pub struct RtpReceiverSocket {
    socket: UdpSocket,
}

impl RtpReceiverSocket {
    /// Binds an IPv4 or IPv6 UDP socket for one RTP media track.
    pub fn bind(local_addr: SocketAddr) -> Result<Self, AirplayError> {
        let socket = UdpSocket::bind(local_addr).map_err(|error| AirplayError::Io(error.kind()))?;
        Ok(Self { socket })
    }

    /// Returns the OS-assigned local address.
    pub fn local_addr(&self) -> Result<SocketAddr, AirplayError> {
        self.socket
            .local_addr()
            .map_err(|error| AirplayError::Io(error.kind()))
    }

    /// Configures the bounded receive deadline used by [`Self::recv`].
    pub fn set_read_timeout(
        &self,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), AirplayError> {
        self.socket
            .set_read_timeout(timeout)
            .map_err(|error| AirplayError::Io(error.kind()))
    }

    /// Receives and parses one RTP datagram, returning its source address.
    pub fn recv(&self) -> Result<(RtpAudioPacket, SocketAddr), AirplayError> {
        let mut buffer = vec![0_u8; MAX_RTP_PACKET_BYTES + 1];
        let (length, source) = self
            .socket
            .recv_from(&mut buffer)
            .map_err(|error| AirplayError::Io(error.kind()))?;
        if length > MAX_RTP_PACKET_BYTES {
            return Err(AirplayError::TooLarge("RTP packet"));
        }
        let packet = RtpAudioPacket::parse(&buffer[..length])?;
        Ok((packet, source))
    }
}

impl std::error::Error for AirplayError {}

/// RTSP request or response start line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RtspStartLine {
    /// An RTSP request sent to a receiver.
    Request {
        /// Method such as `OPTIONS`, `ANNOUNCE`, or `SETUP`.
        method: String,
        /// Absolute or relative RTSP URI.
        uri: String,
    },
    /// An RTSP response returned by a receiver.
    Response {
        /// Numeric status code.
        status: u16,
        /// Short reason phrase.
        reason: String,
    },
}

/// A bounded RTSP/1.0 message with normalized lowercase headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RtspMessage {
    /// Request or response line.
    pub start_line: RtspStartLine,
    /// Lowercase header names in deterministic order.
    pub headers: BTreeMap<String, String>,
    /// Optional bounded body.
    pub body: Vec<u8>,
}

impl RtspMessage {
    /// Parses one complete CRLF-delimited RTSP message.
    pub fn parse(bytes: &[u8]) -> Result<Self, AirplayError> {
        if bytes.len() > MAX_RTSP_BYTES {
            return Err(AirplayError::TooLarge("RTSP message"));
        }
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or(AirplayError::Invalid("RTSP terminator"))?;
        let body = &bytes[header_end + 4..];
        if body.len() > MAX_RTSP_BODY_BYTES {
            return Err(AirplayError::TooLarge("RTSP body"));
        }
        let text = std::str::from_utf8(&bytes[..header_end])
            .map_err(|_| AirplayError::Invalid("RTSP UTF-8"))?;
        let mut lines = text.split("\r\n");
        let start_line = parse_start_line(
            lines
                .next()
                .ok_or(AirplayError::Invalid("RTSP start line"))?,
        )?;
        let mut headers = BTreeMap::new();
        for line in lines {
            let (name, value) = line
                .split_once(':')
                .ok_or(AirplayError::Invalid("RTSP header"))?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(AirplayError::Invalid("RTSP header name"));
            }
            let name = name.to_ascii_lowercase();
            let value = value.trim();
            if value.is_empty()
                || value.len() > 2048
                || headers.insert(name, value.to_string()).is_some()
            {
                return Err(AirplayError::Invalid("RTSP duplicate or oversized header"));
            }
        }
        if let Some(length) = headers.get("content-length") {
            let declared = length
                .parse::<usize>()
                .map_err(|_| AirplayError::Invalid("RTSP content length"))?;
            if declared != body.len() {
                return Err(AirplayError::Invalid("RTSP content length"));
            }
        }
        Ok(Self {
            start_line,
            headers,
            body: body.to_vec(),
        })
    }

    /// Encodes one deterministic RTSP/1.0 message.
    pub fn encode(&self) -> Result<Vec<u8>, AirplayError> {
        if self.body.len() > MAX_RTSP_BODY_BYTES {
            return Err(AirplayError::TooLarge("RTSP body"));
        }
        let start = match &self.start_line {
            RtspStartLine::Request { method, uri } => {
                validate_text(method, "RTSP method")?;
                validate_text(uri, "RTSP URI")?;
                format!("{method} {uri} RTSP/1.0")
            }
            RtspStartLine::Response { status, reason } => {
                if *status < 100 {
                    return Err(AirplayError::Invalid("RTSP status"));
                }
                validate_text(reason, "RTSP reason")?;
                format!("RTSP/1.0 {status} {reason}")
            }
        };
        let mut bytes = start.into_bytes();
        bytes.extend_from_slice(b"\r\n");
        for (name, value) in &self.headers {
            validate_text(name, "RTSP header name")?;
            validate_text(value, "RTSP header value")?;
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(b": ");
            bytes.extend_from_slice(value.as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&self.body);
        if bytes.len() > MAX_RTSP_BYTES {
            return Err(AirplayError::TooLarge("RTSP message"));
        }
        Ok(bytes)
    }

    /// Reads the numeric CSeq header used by RAOP request ordering.
    pub fn cseq(&self) -> Result<u32, AirplayError> {
        self.headers
            .get("cseq")
            .ok_or(AirplayError::Invalid("RTSP CSeq"))?
            .parse()
            .map_err(|_| AirplayError::Invalid("RTSP CSeq"))
    }
}

fn parse_start_line(line: &str) -> Result<RtspStartLine, AirplayError> {
    if line.starts_with("RTSP/1.0 ") {
        let parts = line.splitn(3, ' ').collect::<Vec<_>>();
        if parts.len() != 3 || parts[2].is_empty() {
            return Err(AirplayError::Invalid("RTSP response line"));
        }
        return Ok(RtspStartLine::Response {
            status: parts[1]
                .parse()
                .map_err(|_| AirplayError::Invalid("RTSP status"))?,
            reason: parts[2].to_string(),
        });
    }
    let parts = line.splitn(3, ' ').collect::<Vec<_>>();
    if parts.len() != 3 || parts[2] != "RTSP/1.0" || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AirplayError::Invalid("RTSP request line"));
    }
    Ok(RtspStartLine::Request {
        method: parts[0].to_string(),
        uri: parts[1].to_string(),
    })
}

fn validate_text(value: &str, field: &'static str) -> Result<(), AirplayError> {
    if value.is_empty() || value.len() > 2048 || value.contains(['\r', '\n']) {
        return Err(AirplayError::InvalidField(field));
    }
    Ok(())
}

/// One unencrypted RTP audio packet used by the experimental RAOP boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RtpAudioPacket {
    /// RTP marker bit.
    pub marker: bool,
    /// Payload type negotiated by RTSP.
    pub payload_type: u8,
    /// Monotonic RTP sequence number.
    pub sequence: u16,
    /// RTP sample timestamp.
    pub timestamp: u32,
    /// Synchronization source identifier.
    pub ssrc: u32,
    /// Encoded or PCM audio payload.
    pub payload: Vec<u8>,
}

/// RTP packet envelope reused by the mirror video pipeline.
///
/// The wire header is identical to the audio envelope; the alias keeps the
/// bounded parser and jitter ordering policy shared without claiming a codec.
pub type RtpVideoPacket = RtpAudioPacket;

/// Result of inserting one packet into the bounded RTP jitter buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RtpPushResult {
    /// The packet is retained until its sequence becomes the next packet.
    Buffered,
    /// The packet arrived behind the already-consumed sequence window.
    DroppedLate,
}

/// A sequence-aware, bounded jitter buffer for unencrypted RAOP RTP packets.
///
/// The buffer does not conceal loss or decode audio. Callers can use
/// [`Self::skip_missing_to`] after a protocol-specific deadline to advance over
/// a gap, then continue draining packets in sequence order.
#[derive(Debug)]
pub struct RtpJitterBuffer {
    capacity: usize,
    next_sequence: Option<u16>,
    packets: BTreeMap<u16, RtpAudioPacket>,
    dropped_late: u64,
}

impl RtpJitterBuffer {
    /// Creates a buffer with a bounded packet capacity.
    pub fn new(capacity: usize) -> Result<Self, AirplayError> {
        if capacity == 0 || capacity > MAX_RTP_JITTER_PACKETS {
            return Err(AirplayError::InvalidField("RTP jitter capacity"));
        }
        Ok(Self {
            capacity,
            next_sequence: None,
            packets: BTreeMap::new(),
            dropped_late: 0,
        })
    }

    /// Inserts one packet while preserving sequence-wrap ordering.
    pub fn push(&mut self, packet: RtpAudioPacket) -> Result<RtpPushResult, AirplayError> {
        if let Some(next) = self.next_sequence {
            if sequence_before(packet.sequence, next) {
                self.dropped_late = self.dropped_late.saturating_add(1);
                return Ok(RtpPushResult::DroppedLate);
            }
        } else {
            self.next_sequence = Some(packet.sequence);
        }
        if self.packets.contains_key(&packet.sequence) {
            return Err(AirplayError::Invalid("RTP duplicate sequence"));
        }
        if self.packets.len() >= self.capacity {
            return Err(AirplayError::TooLarge("RTP jitter buffer"));
        }
        self.packets.insert(packet.sequence, packet);
        Ok(RtpPushResult::Buffered)
    }

    /// Removes the packet whose sequence is currently due, if available.
    pub fn pop_ready(&mut self) -> Option<RtpAudioPacket> {
        let sequence = self.next_sequence?;
        let packet = self.packets.remove(&sequence)?;
        self.next_sequence = Some(sequence.wrapping_add(1));
        Some(packet)
    }

    /// Advances across a missing sequence gap and returns the number skipped.
    pub fn skip_missing_to(&mut self, sequence: u16) -> Result<u16, AirplayError> {
        let Some(next) = self.next_sequence else {
            self.next_sequence = Some(sequence);
            return Ok(0);
        };
        if sequence_before(sequence, next) {
            return Err(AirplayError::Invalid("RTP sequence recovery"));
        }
        let missing = sequence.wrapping_sub(next);
        self.packets
            .retain(|packet_sequence, _| !sequence_before(*packet_sequence, sequence));
        self.next_sequence = Some(sequence);
        Ok(missing)
    }

    /// Returns the number of packets waiting in the buffer.
    pub fn buffered_len(&self) -> usize {
        self.packets.len()
    }

    /// Returns the next sequence expected by the consumer.
    pub fn next_sequence(&self) -> Option<u16> {
        self.next_sequence
    }

    /// Returns the number of packets dropped after arriving late.
    pub fn dropped_late(&self) -> u64 {
        self.dropped_late
    }
}

fn sequence_before(sequence: u16, reference: u16) -> bool {
    let distance = sequence.wrapping_sub(reference);
    distance > 0x8000
}

impl RtpAudioPacket {
    /// Parses RTP version 2 without padding or header extensions.
    pub fn parse(bytes: &[u8]) -> Result<Self, AirplayError> {
        if bytes.len() < 12 {
            return Err(AirplayError::Invalid("RTP header"));
        }
        if bytes[0] >> 6 != 2 {
            return Err(AirplayError::Invalid("RTP version"));
        }
        if bytes[0] & 0x20 != 0 || bytes[0] & 0x10 != 0 {
            return Err(AirplayError::Invalid("RTP padding or extension"));
        }
        let csrc_count = usize::from(bytes[0] & 0x0f);
        let header_len = 12usize
            .checked_add(csrc_count.saturating_mul(4))
            .ok_or(AirplayError::Invalid("RTP CSRC count"))?;
        if bytes.len() < header_len {
            return Err(AirplayError::Invalid("RTP CSRC list"));
        }
        let payload = &bytes[header_len..];
        if payload.is_empty() || payload.len() > MAX_RTP_PAYLOAD_BYTES {
            return Err(AirplayError::Invalid("RTP payload"));
        }
        Ok(Self {
            marker: bytes[1] & 0x80 != 0,
            payload_type: bytes[1] & 0x7f,
            sequence: u16::from_be_bytes([bytes[2], bytes[3]]),
            timestamp: u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
            ssrc: u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            payload: payload.to_vec(),
        })
    }

    /// Encodes one RTP version-2 packet with no extension or CSRC list.
    pub fn encode(&self) -> Result<Vec<u8>, AirplayError> {
        if self.payload.is_empty() || self.payload.len() > MAX_RTP_PAYLOAD_BYTES {
            return Err(AirplayError::Invalid("RTP payload"));
        }
        if self.payload_type > 127 {
            return Err(AirplayError::Invalid("RTP payload type"));
        }
        let mut bytes = Vec::with_capacity(12 + self.payload.len());
        bytes.push(0x80);
        bytes.push(self.payload_type | if self.marker { 0x80 } else { 0 });
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.ssrc.to_be_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

/// Bounded mirror-audio RTP adapter that combines sequence recovery with the
/// validated AAC/PCM16 access-unit contract.
///
/// Network sockets, encryption, decoder selection, and clock feedback remain
/// outside this crate. The caller pushes parsed RTP packets, calls
/// [`Self::pop_ready`] in sequence order, and explicitly advances across a
/// missing packet after its transport deadline.
#[derive(Debug)]
pub struct MirrorAudioPipeline {
    format: MirrorAudioFormat,
    payload_type: u8,
    samples_per_packet: u32,
    jitter: RtpJitterBuffer,
}

impl MirrorAudioPipeline {
    /// Creates a pipeline for one negotiated mirror audio payload type.
    pub fn new(
        format: MirrorAudioFormat,
        payload_type: u8,
        samples_per_packet: u32,
        capacity: usize,
    ) -> Result<Self, AirplayError> {
        if payload_type > 127 {
            return Err(AirplayError::Invalid("mirror audio payload type"));
        }
        if samples_per_packet == 0 || samples_per_packet > MAX_MIRROR_AUDIO_DURATION_SAMPLES {
            return Err(AirplayError::Invalid("mirror audio packet duration"));
        }
        Ok(Self {
            format,
            payload_type,
            samples_per_packet,
            jitter: RtpJitterBuffer::new(capacity)?,
        })
    }

    /// Inserts a parsed RTP packet after validating the negotiated payload type.
    pub fn push(&mut self, packet: RtpAudioPacket) -> Result<RtpPushResult, AirplayError> {
        if packet.payload_type != self.payload_type {
            return Err(AirplayError::Invalid("mirror audio payload type"));
        }
        if self.format.codec == MirrorAudioCodec::Pcm16
            && !packet
                .payload
                .len()
                .is_multiple_of(usize::from(self.format.channels) * 2)
        {
            return Err(AirplayError::Invalid("mirror PCM payload"));
        }
        self.jitter.push(packet)
    }

    /// Returns the next sequence-ordered access unit, if one is ready.
    pub fn pop_ready(&mut self) -> Result<Option<MirrorAudioAccessUnit>, AirplayError> {
        let Some(packet) = self.jitter.pop_ready() else {
            return Ok(None);
        };
        let duration_samples = if self.format.codec == MirrorAudioCodec::Pcm16 {
            u32::try_from(packet.payload.len() / (usize::from(self.format.channels) * 2))
                .map_err(|_| AirplayError::Invalid("mirror audio packet duration"))?
        } else {
            self.samples_per_packet
        };
        MirrorAudioAccessUnit::new(
            u64::from(packet.timestamp),
            duration_samples,
            self.format,
            packet.payload,
        )
        .map(Some)
    }

    /// Advances over a missing sequence after the caller's loss deadline.
    pub fn skip_missing_to(&mut self, sequence: u16) -> Result<u16, AirplayError> {
        self.jitter.skip_missing_to(sequence)
    }

    /// Returns the number of packets waiting for ordered delivery.
    pub fn buffered_len(&self) -> usize {
        self.jitter.buffered_len()
    }

    /// Returns the number of packets dropped after arriving late.
    pub fn dropped_late(&self) -> u64 {
        self.jitter.dropped_late()
    }
}

/// Codec names currently understood by the experimental RAOP boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RaopCodec {
    /// Uncompressed big-endian PCM.
    Pcm,
    /// Apple Lossless payload; decoder is not included.
    Alac,
    /// AAC payload; decoder is not included.
    Aac,
}

/// Negotiated RAOP audio parameters from one ANNOUNCE SDP body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RaopAudioFormat {
    /// Codec selected by the sender's RTP map.
    pub codec: RaopCodec,
    /// RTP sample rate in Hz.
    pub sample_rate: u32,
    /// Number of interleaved audio channels.
    pub channels: u8,
    /// Bits per PCM sample when the codec exposes it.
    pub bits_per_sample: u8,
    /// Sender RTP payload type.
    pub payload_type: u8,
}

impl RaopAudioFormat {
    /// Parses the bounded SDP subset used by RAOP ANNOUNCE.
    pub fn from_sdp(body: &[u8]) -> Result<Self, AirplayError> {
        if body.is_empty() || body.len() > MAX_SDP_BYTES {
            return Err(AirplayError::TooLarge("SDP body"));
        }
        let text = std::str::from_utf8(body).map_err(|_| AirplayError::Invalid("SDP UTF-8"))?;
        let mut lines = 0usize;
        let mut media_audio = false;
        let mut payload_type = None;
        let mut format = None;
        for line in text.split(['\r', '\n']).filter(|line| !line.is_empty()) {
            lines = lines.saturating_add(1);
            if lines > MAX_SDP_LINES {
                return Err(AirplayError::TooLarge("SDP lines"));
            }
            if line.len() > 2048 || !line.contains('=') {
                return Err(AirplayError::Invalid("SDP line"));
            }
            if line == "m=audio 0 RTP/AVP 96" || line.starts_with("m=audio ") {
                let parts = line.split_whitespace().collect::<Vec<_>>();
                if parts.len() < 4 || parts[2] != "RTP/AVP" {
                    return Err(AirplayError::Invalid("SDP media description"));
                }
                payload_type = Some(
                    parts[3]
                        .parse::<u8>()
                        .map_err(|_| AirplayError::Invalid("SDP payload type"))?,
                );
                media_audio = true;
            } else if let Some(value) = line.strip_prefix("a=rtpmap:") {
                let (payload, codec_details) = value
                    .split_once(' ')
                    .ok_or(AirplayError::Invalid("SDP rtpmap"))?;
                let payload = payload
                    .parse::<u8>()
                    .map_err(|_| AirplayError::Invalid("SDP payload type"))?;
                let parts = codec_details.split('/').collect::<Vec<_>>();
                if parts.len() < 2 || parts.len() > 3 {
                    return Err(AirplayError::Invalid("SDP rtpmap"));
                }
                let codec = match parts[0].to_ascii_lowercase().as_str() {
                    "applelossless" => RaopCodec::Alac,
                    "mpeg4-generic" => RaopCodec::Aac,
                    "l16" | "pcm" => RaopCodec::Pcm,
                    _ => return Err(AirplayError::Invalid("SDP codec")),
                };
                let sample_rate = parts[1]
                    .parse::<u32>()
                    .map_err(|_| AirplayError::Invalid("SDP sample rate"))?;
                let channels = parts
                    .get(2)
                    .map(|value| {
                        value
                            .parse::<u8>()
                            .map_err(|_| AirplayError::Invalid("SDP channels"))
                    })
                    .transpose()?
                    .unwrap_or(2);
                if sample_rate == 0 || channels == 0 || channels > 8 {
                    return Err(AirplayError::Invalid("SDP audio format"));
                }
                format = Some((payload, codec, sample_rate, channels));
            }
        }
        if !media_audio {
            return Err(AirplayError::Invalid("SDP audio media"));
        }
        let Some((mapped_payload, codec, sample_rate, channels)) = format else {
            return Err(AirplayError::Invalid("SDP rtpmap"));
        };
        if payload_type != Some(mapped_payload) {
            return Err(AirplayError::Invalid("SDP payload mismatch"));
        }
        Ok(Self {
            codec,
            sample_rate,
            channels,
            bits_per_sample: if codec == RaopCodec::Pcm { 16 } else { 0 },
            payload_type: mapped_payload,
        })
    }
}

/// RAOP negotiation state owned by one RTSP connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RaopSessionState {
    /// No ANNOUNCE has been accepted.
    Idle,
    /// Audio format has been negotiated.
    Announced,
    /// UDP transport has been selected.
    Setup,
    /// Sender has started RTP delivery.
    Recording,
    /// Session has been torn down and cannot be reused.
    Closed,
}

/// Bounded RAOP RTSP state machine. It does not own a TCP or UDP socket.
pub struct RaopSession {
    state: RaopSessionState,
    audio_format: Option<RaopAudioFormat>,
    server_port: u16,
    session_id: String,
}

impl RaopSession {
    /// Creates a session with an explicit caller-bound UDP server port.
    pub fn new(server_port: u16) -> Result<Self, AirplayError> {
        if server_port == 0 {
            return Err(AirplayError::InvalidField("RAOP server port"));
        }
        Ok(Self {
            state: RaopSessionState::Idle,
            audio_format: None,
            server_port,
            session_id: "frameark-raop-1".to_string(),
        })
    }

    /// Returns the current negotiation state.
    pub fn state(&self) -> RaopSessionState {
        self.state
    }

    /// Returns the negotiated audio format after ANNOUNCE.
    pub fn audio_format(&self) -> Option<&RaopAudioFormat> {
        self.audio_format.as_ref()
    }

    /// Applies one RTSP request and returns a bounded response.
    pub fn handle(&mut self, request: &RtspMessage) -> Result<RtspMessage, AirplayError> {
        let cseq = request.cseq()?;
        let RtspStartLine::Request { method, .. } = &request.start_line else {
            return Err(AirplayError::Invalid("RTSP request"));
        };
        let method = method.as_str();
        let mut headers = BTreeMap::new();
        headers.insert("cseq".to_string(), cseq.to_string());
        headers.insert("server".to_string(), "FrameArk/0.1 RAOP".to_string());
        if method != "OPTIONS" {
            headers.insert("session".to_string(), self.session_id.clone());
        }
        let body = match method {
            "OPTIONS" => {
                headers.insert(
                    "public".to_string(),
                    "OPTIONS, ANNOUNCE, SETUP, RECORD, FLUSH, TEARDOWN, GET_PARAMETER".to_string(),
                );
                Vec::new()
            }
            "ANNOUNCE" if self.state == RaopSessionState::Idle => {
                let format = RaopAudioFormat::from_sdp(&request.body)?;
                self.audio_format = Some(format);
                self.state = RaopSessionState::Announced;
                Vec::new()
            }
            "SETUP" if self.state == RaopSessionState::Announced => {
                let transport = request
                    .headers
                    .get("transport")
                    .ok_or(AirplayError::Invalid("RAOP transport"))?;
                validate_raop_transport(transport)?;
                headers.insert(
                    "transport".to_string(),
                    format!(
                        "RTP/AVP/UDP;unicast;mode=record;server_port={}",
                        self.server_port
                    ),
                );
                self.state = RaopSessionState::Setup;
                Vec::new()
            }
            "RECORD" if self.state == RaopSessionState::Setup => {
                self.state = RaopSessionState::Recording;
                Vec::new()
            }
            "FLUSH"
                if matches!(
                    self.state,
                    RaopSessionState::Setup | RaopSessionState::Recording
                ) =>
            {
                Vec::new()
            }
            "GET_PARAMETER" if self.state != RaopSessionState::Closed => request.body.clone(),
            "TEARDOWN" if self.state != RaopSessionState::Closed => {
                self.state = RaopSessionState::Closed;
                self.audio_format = None;
                Vec::new()
            }
            _ => return Err(AirplayError::Invalid("RAOP session transition")),
        };
        if !body.is_empty() {
            headers.insert("content-length".to_string(), body.len().to_string());
        }
        Ok(RtspMessage {
            start_line: RtspStartLine::Response {
                status: 200,
                reason: "OK".to_string(),
            },
            headers,
            body,
        })
    }
}

fn validate_raop_transport(value: &str) -> Result<(), AirplayError> {
    validate_text(value, "RAOP transport")?;
    if !value.contains("RTP/AVP/UDP")
        || !value.contains("unicast")
        || !value.contains("mode=record")
        || value.contains("interleaved=")
    {
        return Err(AirplayError::Invalid("RAOP transport"));
    }
    Ok(())
}

/// XML Property List value subset used by AirPlay control metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlistValue {
    /// UTF-8 string value.
    String(String),
    /// Signed integer value.
    Integer(i64),
    /// Boolean value.
    Boolean(bool),
    /// Opaque binary data represented as XML base64.
    Data(Vec<u8>),
    /// Ordered values from an XML array.
    Array(Vec<Self>),
    /// Deterministically ordered key/value dictionary.
    Dictionary(BTreeMap<String, Self>),
}

/// Screen orientation signaled by a mirror session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorOrientation {
    /// Natural orientation.
    Deg0,
    /// Clockwise quarter turn.
    Deg90,
    /// Half turn.
    Deg180,
    /// Counter-clockwise quarter turn.
    Deg270,
}

impl TryFrom<u16> for MirrorOrientation {
    type Error = AirplayError;

    /// Converts a wire degree value while rejecting unsupported angles.
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Deg0),
            90 => Ok(Self::Deg90),
            180 => Ok(Self::Deg180),
            270 => Ok(Self::Deg270),
            _ => Err(AirplayError::Invalid("mirror orientation")),
        }
    }
}

/// One validated H.264 NAL unit in a mirror access unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H264NalUnit {
    /// H.264 nal_unit_type (1..=31).
    pub nal_type: u8,
    /// NAL header and RBSP bytes without an Annex-B start code.
    pub bytes: Vec<u8>,
}

/// A bounded H.264 video access unit with a 90 kHz presentation timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirrorVideoAccessUnit {
    /// Presentation timestamp in `MIRROR_VIDEO_CLOCK_HZ` ticks.
    pub pts_90khz: u64,
    /// Orientation that applies to this access unit.
    pub orientation: MirrorOrientation,
    /// Whether the unit contains an IDR NAL and can start decoding.
    pub keyframe: bool,
    /// NAL units in sender order.
    pub nal_units: Vec<H264NalUnit>,
}

impl MirrorVideoAccessUnit {
    /// Parses Annex-B start-code framing and validates each NAL boundary.
    pub fn from_annex_b(
        pts_90khz: u64,
        orientation: MirrorOrientation,
        bytes: &[u8],
    ) -> Result<Self, AirplayError> {
        if bytes.is_empty() || bytes.len() > MAX_MIRROR_ACCESS_UNIT_BYTES {
            return Err(AirplayError::TooLarge("mirror access unit"));
        }
        let mut units = Vec::new();
        let mut cursor = 0;
        let Some((first, first_len)) = find_start_code(bytes, cursor) else {
            return Err(AirplayError::Invalid("H264 Annex-B start code"));
        };
        if bytes[..first].iter().any(|byte| *byte != 0) {
            return Err(AirplayError::Invalid("H264 Annex-B prefix"));
        }
        cursor = first + first_len;
        while cursor < bytes.len() {
            let next = find_start_code(bytes, cursor);
            let end = next.map(|(index, _)| index).unwrap_or(bytes.len());
            push_h264_nal(&mut units, &bytes[cursor..end])?;
            cursor = next
                .map(|(index, length)| index + length)
                .unwrap_or(bytes.len());
        }
        if units.is_empty() {
            return Err(AirplayError::Invalid("H264 NAL units"));
        }
        let keyframe = units.iter().any(|unit| unit.nal_type == 5);
        Ok(Self {
            pts_90khz,
            orientation,
            keyframe,
            nal_units: units,
        })
    }

    /// Parses AVCC length-prefixed NAL units used by some mirror transports.
    pub fn from_avcc(
        pts_90khz: u64,
        orientation: MirrorOrientation,
        bytes: &[u8],
    ) -> Result<Self, AirplayError> {
        if bytes.is_empty() || bytes.len() > MAX_MIRROR_ACCESS_UNIT_BYTES {
            return Err(AirplayError::TooLarge("mirror access unit"));
        }
        let mut units = Vec::new();
        let mut cursor = 0;
        while cursor < bytes.len() {
            let header_end = cursor
                .checked_add(4)
                .ok_or(AirplayError::Invalid("H264 AVCC length"))?;
            if header_end > bytes.len() {
                return Err(AirplayError::Invalid("H264 AVCC length"));
            }
            let length = u32::from_be_bytes(bytes[cursor..header_end].try_into().unwrap()) as usize;
            cursor = header_end;
            let end = cursor
                .checked_add(length)
                .ok_or(AirplayError::Invalid("H264 AVCC length"))?;
            if end > bytes.len() {
                return Err(AirplayError::Invalid("H264 AVCC length"));
            }
            push_h264_nal(&mut units, &bytes[cursor..end])?;
            cursor = end;
        }
        if units.is_empty() {
            return Err(AirplayError::Invalid("H264 NAL units"));
        }
        let keyframe = units.iter().any(|unit| unit.nal_type == 5);
        Ok(Self {
            pts_90khz,
            orientation,
            keyframe,
            nal_units: units,
        })
    }

    /// Re-encodes the unit as four-byte Annex-B start-code NALs.
    pub fn to_annex_b(&self) -> Result<Vec<u8>, AirplayError> {
        if self.nal_units.is_empty() || self.nal_units.len() > MAX_MIRROR_NAL_UNITS {
            return Err(AirplayError::Invalid("H264 NAL units"));
        }
        let total = self
            .nal_units
            .iter()
            .try_fold(0usize, |total, unit| {
                total.checked_add(4 + unit.bytes.len())
            })
            .ok_or(AirplayError::TooLarge("mirror access unit"))?;
        if total > MAX_MIRROR_ACCESS_UNIT_BYTES {
            return Err(AirplayError::TooLarge("mirror access unit"));
        }
        let mut output = Vec::with_capacity(total);
        for unit in &self.nal_units {
            output.extend_from_slice(&[0, 0, 0, 1]);
            output.extend_from_slice(&unit.bytes);
        }
        Ok(output)
    }
}

#[derive(Debug)]
struct MirrorVideoAssembly {
    timestamp: u32,
    bytes: usize,
    nal_units: Vec<H264NalUnit>,
}

#[derive(Debug)]
struct MirrorVideoFragment {
    timestamp: u32,
    bytes: Vec<u8>,
}

/// Bounded H.264-over-RTP mirror video adapter.
///
/// The pipeline accepts single-NAL, STAP-A, and FU-A payloads from a caller-
/// parsed RTP stream. Packets are reordered through the shared jitter buffer;
/// the RTP marker closes one access unit. A missing packet must be declared by
/// the caller with [`Self::skip_missing_to`], which discards any partial access
/// unit rather than silently manufacturing a decodable frame.
#[derive(Debug)]
pub struct MirrorVideoPipeline {
    orientation: MirrorOrientation,
    payload_type: u8,
    jitter: RtpJitterBuffer,
    assembly: Option<MirrorVideoAssembly>,
    fragment: Option<MirrorVideoFragment>,
}

impl MirrorVideoPipeline {
    /// Creates a pipeline for one negotiated mirror payload type and jitter
    /// capacity.
    pub fn new(
        orientation: MirrorOrientation,
        payload_type: u8,
        capacity: usize,
    ) -> Result<Self, AirplayError> {
        if payload_type > 127 {
            return Err(AirplayError::Invalid("mirror video payload type"));
        }
        Ok(Self {
            orientation,
            payload_type,
            jitter: RtpJitterBuffer::new(capacity)?,
            assembly: None,
            fragment: None,
        })
    }

    /// Updates the orientation applied to the next completed access unit.
    pub fn set_orientation(&mut self, orientation: MirrorOrientation) {
        self.orientation = orientation;
    }

    /// Inserts one parsed RTP packet after validating the negotiated payload.
    pub fn push(&mut self, packet: RtpVideoPacket) -> Result<RtpPushResult, AirplayError> {
        if packet.payload_type != self.payload_type {
            return Err(AirplayError::Invalid("mirror video payload type"));
        }
        self.jitter.push(packet)
    }

    /// Returns the next complete marker-delimited H.264 access unit.
    pub fn pop_ready(&mut self) -> Result<Option<MirrorVideoAccessUnit>, AirplayError> {
        loop {
            let Some(packet) = self.jitter.pop_ready() else {
                return Ok(None);
            };
            self.consume_packet(&packet)?;
            if !packet.marker {
                continue;
            }
            if self.fragment.is_some() {
                return Err(AirplayError::Invalid("mirror video FU-A marker"));
            }
            let assembly = self
                .assembly
                .take()
                .ok_or(AirplayError::Invalid("mirror video access unit"))?;
            if assembly.nal_units.is_empty() {
                return Err(AirplayError::Invalid("mirror video NAL units"));
            }
            let keyframe = assembly.nal_units.iter().any(|unit| unit.nal_type == 5);
            return Ok(Some(MirrorVideoAccessUnit {
                pts_90khz: u64::from(assembly.timestamp),
                orientation: self.orientation,
                keyframe,
                nal_units: assembly.nal_units,
            }));
        }
    }

    /// Advances over a missing sequence and discards any partial video unit.
    pub fn skip_missing_to(&mut self, sequence: u16) -> Result<u16, AirplayError> {
        let skipped = self.jitter.skip_missing_to(sequence)?;
        if skipped != 0 {
            self.assembly = None;
            self.fragment = None;
        }
        Ok(skipped)
    }

    /// Returns the number of packets waiting for ordered delivery.
    pub fn buffered_len(&self) -> usize {
        self.jitter.buffered_len()
    }

    /// Returns the number of packets dropped after arriving late.
    pub fn dropped_late(&self) -> u64 {
        self.jitter.dropped_late()
    }

    fn consume_packet(&mut self, packet: &RtpVideoPacket) -> Result<(), AirplayError> {
        if let Some(assembly) = &self.assembly {
            if assembly.timestamp != packet.timestamp {
                return Err(AirplayError::Invalid("mirror video timestamp boundary"));
            }
        } else {
            self.assembly = Some(MirrorVideoAssembly {
                timestamp: packet.timestamp,
                bytes: 0,
                nal_units: Vec::new(),
            });
        }
        let payload = packet.payload.as_slice();
        if payload.is_empty() {
            return Err(AirplayError::Invalid("mirror video RTP payload"));
        }
        match payload[0] & 0x1f {
            1..=23 => self.append_nal(payload),
            24 => self.consume_stap_a(payload),
            28 => self.consume_fu_a(packet.timestamp, payload),
            _ => Err(AirplayError::Invalid("mirror video RTP NAL type")),
        }
    }

    fn consume_stap_a(&mut self, payload: &[u8]) -> Result<(), AirplayError> {
        let mut cursor = 1usize;
        let mut count = 0usize;
        while cursor < payload.len() {
            let end = cursor
                .checked_add(2)
                .ok_or(AirplayError::Invalid("mirror video STAP-A length"))?;
            if end > payload.len() {
                return Err(AirplayError::Invalid("mirror video STAP-A length"));
            }
            let length = usize::from(u16::from_be_bytes([payload[cursor], payload[cursor + 1]]));
            cursor = end;
            let nal_end = cursor
                .checked_add(length)
                .ok_or(AirplayError::Invalid("mirror video STAP-A length"))?;
            if length == 0 || nal_end > payload.len() {
                return Err(AirplayError::Invalid("mirror video STAP-A NAL"));
            }
            self.append_nal(&payload[cursor..nal_end])?;
            count = count.saturating_add(1);
            cursor = nal_end;
        }
        if count == 0 {
            return Err(AirplayError::Invalid("mirror video STAP-A NAL"));
        }
        Ok(())
    }

    fn consume_fu_a(&mut self, timestamp: u32, payload: &[u8]) -> Result<(), AirplayError> {
        if payload.len() < 3 {
            return Err(AirplayError::Invalid("mirror video FU-A header"));
        }
        let indicator = payload[0];
        let header = payload[1];
        let start = header & 0x80 != 0;
        let end = header & 0x40 != 0;
        let nal_type = header & 0x1f;
        if nal_type == 0 || payload[2..].is_empty() || (start && end) {
            return Err(AirplayError::Invalid("mirror video FU-A header"));
        }
        if start {
            if self.fragment.is_some() {
                return Err(AirplayError::Invalid("mirror video FU-A overlap"));
            }
            let mut bytes = Vec::with_capacity(payload.len() - 1);
            bytes.push((indicator & 0xe0) | nal_type);
            bytes.extend_from_slice(&payload[2..]);
            if bytes.len() > MAX_MIRROR_VIDEO_ASSEMBLY_BYTES {
                return Err(AirplayError::TooLarge("mirror video FU-A"));
            }
            self.fragment = Some(MirrorVideoFragment { timestamp, bytes });
            return Ok(());
        }
        let fragment = self
            .fragment
            .as_mut()
            .ok_or(AirplayError::Invalid("mirror video FU-A start"))?;
        if fragment.timestamp != timestamp {
            return Err(AirplayError::Invalid("mirror video FU-A timestamp"));
        }
        let new_len = fragment
            .bytes
            .len()
            .checked_add(payload.len() - 2)
            .ok_or(AirplayError::TooLarge("mirror video FU-A"))?;
        if new_len > MAX_MIRROR_VIDEO_ASSEMBLY_BYTES {
            return Err(AirplayError::TooLarge("mirror video FU-A"));
        }
        fragment.bytes.extend_from_slice(&payload[2..]);
        if end {
            let bytes = self
                .fragment
                .take()
                .ok_or(AirplayError::Invalid("mirror video FU-A end"))?
                .bytes;
            self.append_nal(&bytes)?;
        }
        Ok(())
    }

    fn append_nal(&mut self, bytes: &[u8]) -> Result<(), AirplayError> {
        let Some(assembly) = self.assembly.as_mut() else {
            return Err(AirplayError::Invalid("mirror video access unit"));
        };
        let new_len = assembly
            .bytes
            .checked_add(bytes.len())
            .ok_or(AirplayError::TooLarge("mirror video access unit"))?;
        if new_len > MAX_MIRROR_VIDEO_ASSEMBLY_BYTES {
            return Err(AirplayError::TooLarge("mirror video access unit"));
        }
        push_h264_nal(&mut assembly.nal_units, bytes)?;
        assembly.bytes = new_len;
        Ok(())
    }
}

fn find_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= bytes.len() {
        if bytes[index..].starts_with(&[0, 0, 1]) {
            return Some((index, 3));
        }
        if index + 4 <= bytes.len() && bytes[index..].starts_with(&[0, 0, 0, 1]) {
            return Some((index, 4));
        }
        index += 1;
    }
    None
}

fn push_h264_nal(units: &mut Vec<H264NalUnit>, bytes: &[u8]) -> Result<(), AirplayError> {
    if units.len() >= MAX_MIRROR_NAL_UNITS {
        return Err(AirplayError::TooLarge("H264 NAL units"));
    }
    if bytes.is_empty() || bytes[0] & 0x80 != 0 {
        return Err(AirplayError::Invalid("H264 NAL header"));
    }
    let nal_type = bytes[0] & 0x1f;
    if nal_type == 0 {
        return Err(AirplayError::Invalid("H264 NAL type"));
    }
    units.push(H264NalUnit {
        nal_type,
        bytes: bytes.to_vec(),
    });
    Ok(())
}

/// Bounded video/audio clock conversion policy for mirror synchronization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirrorClock {
    /// Sender audio sample rate in Hz.
    pub audio_sample_rate: u32,
    /// Maximum tolerated signed A/V offset in 90 kHz ticks.
    pub max_offset_90khz: i64,
}

impl MirrorClock {
    /// Creates a clock policy for a supported audio sample rate.
    pub fn new(audio_sample_rate: u32) -> Result<Self, AirplayError> {
        if !(8_000..=192_000).contains(&audio_sample_rate) {
            return Err(AirplayError::Invalid("mirror audio sample rate"));
        }
        Ok(Self {
            audio_sample_rate,
            max_offset_90khz: i64::from(MIRROR_VIDEO_CLOCK_HZ),
        })
    }

    /// Converts an audio sample timestamp to the mirror video clock.
    pub fn audio_samples_to_video_ticks(&self, samples: u64) -> Result<u64, AirplayError> {
        samples
            .checked_mul(u64::from(MIRROR_VIDEO_CLOCK_HZ))
            .and_then(|value| value.checked_div(u64::from(self.audio_sample_rate)))
            .ok_or(AirplayError::TooLarge("mirror timestamp"))
    }

    /// Returns the signed video-minus-audio clock offset when within policy.
    pub fn offset_90khz(&self, video_pts: u64, audio_samples: u64) -> Result<i64, AirplayError> {
        let audio_pts = self.audio_samples_to_video_ticks(audio_samples)?;
        let offset = i128::from(video_pts) - i128::from(audio_pts);
        if offset.unsigned_abs() > self.max_offset_90khz.unsigned_abs() as u128 {
            return Err(AirplayError::Invalid("mirror A/V offset"));
        }
        i64::try_from(offset).map_err(|_| AirplayError::Invalid("mirror A/V offset"))
    }
}

/// States in the bounded AirPlay screen-mirroring RTSP contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorSessionState {
    /// No SETUP has been accepted.
    Idle,
    /// UDP transport and mirror configuration are negotiated.
    Setup,
    /// RECORD has started the mirror media flow.
    Streaming,
    /// TEARDOWN permanently closes the session.
    Closed,
}

/// UDP ports negotiated for a mirror session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirrorTransport {
    /// Receiver port for encoded mirror media.
    pub server_port: u16,
    /// Sender control port.
    pub control_port: u16,
    /// Optional sender timing port.
    pub timing_port: Option<u16>,
}

/// Bounded video/audio parameters carried by a mirror setup Property List.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirrorVideoConfig {
    /// Coded video width in pixels.
    pub width: u16,
    /// Coded video height in pixels.
    pub height: u16,
    /// Orientation applied before platform rendering.
    pub orientation: MirrorOrientation,
    /// Audio sample rate used for the A/V clock policy.
    pub audio_sample_rate: u32,
}

/// Encoded audio format labels accepted by the mirror media contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirrorAudioCodec {
    /// AAC access units remain encoded until a platform decoder consumes them.
    Aac,
    /// Signed little-endian PCM16 samples.
    Pcm16,
}

/// Validated audio format metadata for one mirror track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirrorAudioFormat {
    /// Encoded audio codec.
    pub codec: MirrorAudioCodec,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Interleaved channel count.
    pub channels: u8,
}

impl MirrorAudioFormat {
    /// Creates a format after applying shared sample-rate/channel bounds.
    pub fn new(
        codec: MirrorAudioCodec,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Self, AirplayError> {
        if !(8_000..=192_000).contains(&sample_rate) || !(1..=8).contains(&channels) {
            return Err(AirplayError::Invalid("mirror audio format"));
        }
        Ok(Self {
            codec,
            sample_rate,
            channels,
        })
    }
}

/// One bounded mirror audio access unit with an audio-sample timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirrorAudioAccessUnit {
    /// Audio sample timestamp in the format's sample clock.
    pub pts_samples: u64,
    /// Number of samples represented by this access unit.
    pub duration_samples: u32,
    /// Format used to interpret the payload.
    pub format: MirrorAudioFormat,
    /// Encoded or PCM16 payload bytes.
    pub payload: Vec<u8>,
}

impl MirrorAudioAccessUnit {
    /// Creates one bounded access unit without decoding or copying platform data.
    pub fn new(
        pts_samples: u64,
        duration_samples: u32,
        format: MirrorAudioFormat,
        payload: Vec<u8>,
    ) -> Result<Self, AirplayError> {
        if duration_samples == 0 || duration_samples > MAX_MIRROR_AUDIO_DURATION_SAMPLES {
            return Err(AirplayError::Invalid("mirror audio duration"));
        }
        if payload.is_empty() || payload.len() > MAX_MIRROR_AUDIO_ACCESS_UNIT_BYTES {
            return Err(AirplayError::TooLarge("mirror audio access unit"));
        }
        if format.codec == MirrorAudioCodec::Pcm16
            && !payload
                .len()
                .is_multiple_of(usize::from(format.channels) * 2)
        {
            return Err(AirplayError::Invalid("mirror PCM payload"));
        }
        Ok(Self {
            pts_samples,
            duration_samples,
            format,
            payload,
        })
    }

    /// Returns the audio sample timestamp immediately after this unit.
    pub fn end_samples(&self) -> Result<u64, AirplayError> {
        self.pts_samples
            .checked_add(u64::from(self.duration_samples))
            .ok_or(AirplayError::TooLarge("mirror audio timestamp"))
    }
}

impl MirrorVideoConfig {
    /// Extracts the required bounded fields from an XML or binary plist value.
    pub fn from_plist(value: &PlistValue) -> Result<Self, AirplayError> {
        let PlistValue::Dictionary(dictionary) = value else {
            return Err(AirplayError::Invalid("mirror configuration"));
        };
        let width = plist_unsigned(dictionary, "width")?;
        let height = plist_unsigned(dictionary, "height")?;
        let orientation_value = plist_unsigned(dictionary, "orientation")?;
        if orientation_value > u64::from(u16::MAX) {
            return Err(AirplayError::Invalid("mirror orientation"));
        }
        let orientation = MirrorOrientation::try_from(orientation_value as u16)?;
        let audio_sample_rate = u32::try_from(plist_unsigned(dictionary, "audio_sample_rate")?)
            .map_err(|_| AirplayError::Invalid("mirror audio sample rate"))?;
        if width == 0
            || height == 0
            || width > u64::from(MAX_MIRROR_DIMENSION)
            || height > u64::from(MAX_MIRROR_DIMENSION)
        {
            return Err(AirplayError::Invalid("mirror dimensions"));
        }
        MirrorClock::new(audio_sample_rate)?;
        Ok(Self {
            width: width as u16,
            height: height as u16,
            orientation,
            audio_sample_rate,
        })
    }
}

fn plist_unsigned(
    dictionary: &BTreeMap<String, PlistValue>,
    key: &str,
) -> Result<u64, AirplayError> {
    let Some(PlistValue::Integer(value)) = dictionary.get(key) else {
        return Err(AirplayError::Invalid("mirror configuration field"));
    };
    u64::try_from(*value).map_err(|_| AirplayError::Invalid("mirror configuration field"))
}

/// Bounded RTSP state machine for one AirPlay screen-mirroring session.
///
/// The session owns protocol state only. It does not open UDP sockets, decode
/// H.264, decrypt audio, or render to a platform surface.
pub struct MirrorSession {
    session_id: String,
    server_port: u16,
    state: MirrorSessionState,
    transport: Option<MirrorTransport>,
    config: Option<MirrorVideoConfig>,
}

impl MirrorSession {
    /// Creates an idle session with a caller-selected receiver media port.
    pub fn new(session_id: impl Into<String>, server_port: u16) -> Result<Self, AirplayError> {
        let session_id = session_id.into();
        if session_id.is_empty() || session_id.len() > MAX_MIRROR_SESSION_ID_BYTES {
            return Err(AirplayError::InvalidField("mirror session ID"));
        }
        if server_port == 0 {
            return Err(AirplayError::InvalidField("mirror server port"));
        }
        validate_text(&session_id, "mirror session ID")?;
        Ok(Self {
            session_id,
            server_port,
            state: MirrorSessionState::Idle,
            transport: None,
            config: None,
        })
    }

    /// Returns the current RTSP state.
    pub fn state(&self) -> MirrorSessionState {
        self.state
    }

    /// Returns the negotiated transport after SETUP.
    pub fn transport(&self) -> Option<MirrorTransport> {
        self.transport
    }

    /// Returns the validated mirror configuration after SETUP.
    pub fn config(&self) -> Option<MirrorVideoConfig> {
        self.config
    }

    /// Applies one RTSP request and returns a deterministic response.
    pub fn handle(&mut self, request: &RtspMessage) -> Result<RtspMessage, AirplayError> {
        let cseq = request.cseq()?;
        let RtspStartLine::Request { method, .. } = &request.start_line else {
            return Err(AirplayError::Invalid("mirror RTSP request"));
        };
        let method = method.to_ascii_uppercase();
        let mut response_headers = BTreeMap::new();
        response_headers.insert("cseq".to_string(), cseq.to_string());
        response_headers.insert("session".to_string(), self.session_id.clone());
        let body = match method.as_str() {
            "OPTIONS" if self.state != MirrorSessionState::Closed => {
                response_headers.insert(
                    "public".to_string(),
                    "OPTIONS, SETUP, RECORD, GET_PARAMETER, FLUSH, TEARDOWN".to_string(),
                );
                Vec::new()
            }
            "SETUP" if self.state == MirrorSessionState::Idle => {
                let transport = request
                    .headers
                    .get("transport")
                    .ok_or(AirplayError::Invalid("mirror transport"))?;
                let transport = parse_mirror_transport(transport, self.server_port)?;
                let plist = if request.body.starts_with(b"bplist00") {
                    parse_binary_plist(&request.body)?
                } else {
                    parse_xml_plist(&request.body)?
                };
                let config = MirrorVideoConfig::from_plist(&plist)?;
                self.transport = Some(transport);
                self.config = Some(config);
                self.state = MirrorSessionState::Setup;
                response_headers.insert(
                    "transport".to_string(),
                    format!(
                        "RTP/AVP/UDP;unicast;mode=record;server_port={};control_port={}",
                        transport.server_port, transport.control_port
                    ),
                );
                Vec::new()
            }
            "RECORD" if self.state == MirrorSessionState::Setup => {
                self.state = MirrorSessionState::Streaming;
                Vec::new()
            }
            "GET_PARAMETER" if self.state != MirrorSessionState::Closed => request.body.clone(),
            "FLUSH" if self.state == MirrorSessionState::Streaming => Vec::new(),
            "TEARDOWN" if self.state != MirrorSessionState::Closed => {
                self.state = MirrorSessionState::Closed;
                Vec::new()
            }
            _ => return Err(AirplayError::Invalid("mirror RTSP transition")),
        };
        Ok(RtspMessage {
            start_line: RtspStartLine::Response {
                status: 200,
                reason: "OK".to_string(),
            },
            headers: response_headers,
            body,
        })
    }
}

fn parse_mirror_transport(value: &str, server_port: u16) -> Result<MirrorTransport, AirplayError> {
    validate_text(value, "mirror transport")?;
    let mut control_port = None;
    let mut timing_port = None;
    let mut valid_base = false;
    for part in value.split(';') {
        let part = part.trim();
        if part == "RTP/AVP/UDP" {
            valid_base = true;
        } else if part == "unicast" || part == "mode=record" {
            continue;
        } else if let Some(port) = part.strip_prefix("control_port=") {
            if control_port.is_some() {
                return Err(AirplayError::Invalid("mirror transport"));
            }
            control_port = Some(parse_udp_port(port)?);
        } else if let Some(port) = part.strip_prefix("timing_port=") {
            if timing_port.is_some() {
                return Err(AirplayError::Invalid("mirror transport"));
            }
            timing_port = Some(parse_udp_port(port)?);
        } else {
            return Err(AirplayError::Invalid("mirror transport"));
        }
    }
    if !valid_base {
        return Err(AirplayError::Invalid("mirror transport"));
    }
    Ok(MirrorTransport {
        server_port,
        control_port: control_port.ok_or(AirplayError::Invalid("mirror transport"))?,
        timing_port,
    })
}

fn parse_udp_port(value: &str) -> Result<u16, AirplayError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| AirplayError::Invalid("mirror UDP port"))?;
    if port == 0 {
        return Err(AirplayError::Invalid("mirror UDP port"));
    }
    Ok(port)
}

/// Parses the bounded XML Property List subset into a Rust value.
pub fn parse_xml_plist(bytes: &[u8]) -> Result<PlistValue, AirplayError> {
    if bytes.is_empty() || bytes.len() > MAX_PLIST_BYTES {
        return Err(AirplayError::TooLarge("plist document"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| AirplayError::Invalid("plist UTF-8"))?;
    let start = text
        .find("<plist")
        .ok_or(AirplayError::Invalid("plist root"))?;
    let mut cursor = PlistCursor {
        input: text,
        position: start,
        entries: 0,
    };
    cursor.consume_open("plist")?;
    let value = cursor.parse_value(0)?;
    cursor.consume_close("plist")?;
    if !cursor.input[cursor.position..].trim().is_empty() {
        return Err(AirplayError::Invalid("plist trailing data"));
    }
    validate_plist_value(&value, 0, &mut 0)?;
    Ok(value)
}

/// Parses the bounded binary Property List subset used by newer AirPlay
/// control messages. Strings, integers, booleans, data, arrays, and
/// dictionaries map to [`PlistValue`]. UID, date, real, and null objects are
/// rejected because they have no lossless representation in the shared model.
pub fn parse_binary_plist(bytes: &[u8]) -> Result<PlistValue, AirplayError> {
    if bytes.len() < 8 + 32 || bytes.len() > MAX_PLIST_BYTES {
        return Err(AirplayError::TooLarge("binary plist document"));
    }
    if &bytes[..8] != b"bplist00" {
        return Err(AirplayError::Invalid("binary plist header"));
    }
    let trailer = bytes.len() - 32;
    let offset_int_size = usize::from(bytes[trailer + 6]);
    let object_ref_size = usize::from(bytes[trailer + 7]);
    if !matches!(offset_int_size, 1 | 2 | 4 | 8) || !matches!(object_ref_size, 1 | 2 | 4 | 8) {
        return Err(AirplayError::Invalid("binary plist integer size"));
    }
    let object_count = read_u64(&bytes[trailer + 8..trailer + 16])?;
    let top_object = read_u64(&bytes[trailer + 16..trailer + 24])?;
    let offset_table = read_u64(&bytes[trailer + 24..trailer + 32])?;
    let object_count = usize::try_from(object_count)
        .map_err(|_| AirplayError::TooLarge("binary plist objects"))?;
    let top_object = usize::try_from(top_object)
        .map_err(|_| AirplayError::Invalid("binary plist top object"))?;
    let offset_table = usize::try_from(offset_table)
        .map_err(|_| AirplayError::Invalid("binary plist offset table"))?;
    if object_count == 0
        || object_count > MAX_BINARY_PLIST_OBJECTS
        || top_object >= object_count
        || offset_table < 8
        || offset_table > trailer
    {
        return Err(AirplayError::Invalid("binary plist trailer"));
    }
    let table_bytes = object_count
        .checked_mul(offset_int_size)
        .ok_or(AirplayError::TooLarge("binary plist offset table"))?;
    let table_end = offset_table
        .checked_add(table_bytes)
        .ok_or(AirplayError::TooLarge("binary plist offset table"))?;
    if table_end > trailer {
        return Err(AirplayError::Invalid("binary plist offset table"));
    }
    let mut offsets = Vec::with_capacity(object_count);
    for chunk in bytes[offset_table..table_end].chunks_exact(offset_int_size) {
        let offset = read_uint(chunk)?;
        if offset < 8 || offset >= offset_table {
            return Err(AirplayError::Invalid("binary plist object offset"));
        }
        offsets.push(offset);
    }
    let mut cache = vec![None; object_count];
    let mut visiting = vec![false; object_count];
    parse_binary_object(
        top_object,
        0,
        bytes,
        offset_table,
        object_ref_size,
        &offsets,
        &mut cache,
        &mut visiting,
    )
}

// The parser keeps its bounded state explicit so recursive calls cannot hide
// limits in a global or unbounded heap context.
#[allow(clippy::too_many_arguments)]
fn parse_binary_object(
    index: usize,
    depth: usize,
    bytes: &[u8],
    object_end: usize,
    object_ref_size: usize,
    offsets: &[usize],
    cache: &mut [Option<PlistValue>],
    visiting: &mut [bool],
) -> Result<PlistValue, AirplayError> {
    if depth > MAX_PLIST_DEPTH {
        return Err(AirplayError::TooLarge("binary plist nesting"));
    }
    if let Some(value) = &cache[index] {
        return Ok(value.clone());
    }
    if visiting[index] {
        return Err(AirplayError::Invalid("binary plist cycle"));
    }
    visiting[index] = true;
    let result = parse_binary_object_inner(
        index,
        depth,
        bytes,
        object_end,
        object_ref_size,
        offsets,
        cache,
        visiting,
    );
    visiting[index] = false;
    if let Ok(value) = &result {
        cache[index] = Some(value.clone());
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn parse_binary_object_inner(
    index: usize,
    depth: usize,
    bytes: &[u8],
    object_end: usize,
    object_ref_size: usize,
    offsets: &[usize],
    cache: &mut [Option<PlistValue>],
    visiting: &mut [bool],
) -> Result<PlistValue, AirplayError> {
    let offset = offsets[index];
    let marker = *bytes
        .get(offset)
        .ok_or(AirplayError::Invalid("binary plist object"))?;
    let kind = marker >> 4;
    let info = marker & 0x0f;
    let mut cursor = offset + 1;
    match kind {
        0x0 => match info {
            0x8 => Ok(PlistValue::Boolean(false)),
            0x9 => Ok(PlistValue::Boolean(true)),
            _ => Err(AirplayError::Invalid("binary plist null")),
        },
        0x1 => Ok(PlistValue::Integer(read_binary_integer(
            bytes,
            &mut cursor,
            info,
            object_end,
        )?)),
        0x4 => {
            let length = read_binary_count(bytes, &mut cursor, info, object_end)?;
            if length > MAX_PLIST_FIELD_BYTES
                || cursor
                    .checked_add(length)
                    .is_none_or(|end| end > object_end)
            {
                return Err(AirplayError::TooLarge("binary plist data"));
            }
            Ok(PlistValue::Data(bytes[cursor..cursor + length].to_vec()))
        }
        0x5 => {
            let length = read_binary_count(bytes, &mut cursor, info, object_end)?;
            if length > MAX_PLIST_FIELD_BYTES
                || cursor
                    .checked_add(length)
                    .is_none_or(|end| end > object_end)
            {
                return Err(AirplayError::TooLarge("binary plist string"));
            }
            let value = std::str::from_utf8(&bytes[cursor..cursor + length])
                .map_err(|_| AirplayError::Invalid("binary plist string"))?;
            Ok(PlistValue::String(value.to_string()))
        }
        0x6 => {
            let length = read_binary_count(bytes, &mut cursor, info, object_end)?;
            let byte_length = length
                .checked_mul(2)
                .ok_or(AirplayError::TooLarge("binary plist string"))?;
            if length > MAX_PLIST_FIELD_BYTES
                || cursor
                    .checked_add(byte_length)
                    .is_none_or(|end| end > object_end)
            {
                return Err(AirplayError::TooLarge("binary plist string"));
            }
            let units = bytes[cursor..cursor + byte_length]
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            let value = String::from_utf16(&units)
                .map_err(|_| AirplayError::Invalid("binary plist UTF-16"))?;
            Ok(PlistValue::String(value))
        }
        0xa => {
            let count = read_binary_count(bytes, &mut cursor, info, object_end)?;
            if count > MAX_PLIST_ENTRIES {
                return Err(AirplayError::TooLarge("binary plist entries"));
            }
            let refs_end = cursor
                .checked_add(
                    count
                        .checked_mul(object_ref_size)
                        .ok_or(AirplayError::TooLarge("binary plist references"))?,
                )
                .ok_or(AirplayError::TooLarge("binary plist references"))?;
            if refs_end > object_end {
                return Err(AirplayError::Invalid("binary plist references"));
            }
            let mut values = Vec::with_capacity(count);
            for chunk in bytes[cursor..refs_end].chunks_exact(object_ref_size) {
                let reference = read_uint(chunk)?;
                if reference >= offsets.len() {
                    return Err(AirplayError::Invalid("binary plist reference"));
                }
                values.push(parse_binary_object(
                    reference,
                    depth + 1,
                    bytes,
                    object_end,
                    object_ref_size,
                    offsets,
                    cache,
                    visiting,
                )?);
            }
            Ok(PlistValue::Array(values))
        }
        0xd => {
            let count = read_binary_count(bytes, &mut cursor, info, object_end)?;
            if count > MAX_PLIST_ENTRIES {
                return Err(AirplayError::TooLarge("binary plist entries"));
            }
            let refs_len = count
                .checked_mul(object_ref_size)
                .ok_or(AirplayError::TooLarge("binary plist references"))?;
            let keys_end = cursor
                .checked_add(refs_len)
                .ok_or(AirplayError::TooLarge("binary plist references"))?;
            let values_end = keys_end
                .checked_add(refs_len)
                .ok_or(AirplayError::TooLarge("binary plist references"))?;
            if values_end > object_end {
                return Err(AirplayError::Invalid("binary plist references"));
            }
            let key_refs = bytes[cursor..keys_end]
                .chunks_exact(object_ref_size)
                .map(read_uint)
                .collect::<Result<Vec<_>, _>>()?;
            let value_refs = bytes[keys_end..values_end]
                .chunks_exact(object_ref_size)
                .map(read_uint)
                .collect::<Result<Vec<_>, _>>()?;
            let mut dictionary = BTreeMap::new();
            for (key_ref, value_ref) in key_refs.into_iter().zip(value_refs) {
                if key_ref >= offsets.len() || value_ref >= offsets.len() {
                    return Err(AirplayError::Invalid("binary plist reference"));
                }
                let key = parse_binary_object(
                    key_ref,
                    depth + 1,
                    bytes,
                    object_end,
                    object_ref_size,
                    offsets,
                    cache,
                    visiting,
                )?;
                let PlistValue::String(key) = key else {
                    return Err(AirplayError::Invalid("binary plist dictionary key"));
                };
                let value = parse_binary_object(
                    value_ref,
                    depth + 1,
                    bytes,
                    object_end,
                    object_ref_size,
                    offsets,
                    cache,
                    visiting,
                )?;
                if dictionary.insert(key, value).is_some() {
                    return Err(AirplayError::Invalid("binary plist duplicate key"));
                }
            }
            Ok(PlistValue::Dictionary(dictionary))
        }
        _ => Err(AirplayError::Invalid("binary plist object type")),
    }
}

fn read_binary_count(
    bytes: &[u8],
    cursor: &mut usize,
    info: u8,
    object_end: usize,
) -> Result<usize, AirplayError> {
    if info != 0x0f {
        return Ok(usize::from(info));
    }
    let marker = *bytes
        .get(*cursor)
        .ok_or(AirplayError::Invalid("binary plist length"))?;
    if marker >> 4 != 0x1 {
        return Err(AirplayError::Invalid("binary plist length"));
    }
    let integer_info = marker & 0x0f;
    let value = read_binary_integer(bytes, cursor, integer_info, object_end)?;
    usize::try_from(value).map_err(|_| AirplayError::Invalid("binary plist length"))
}

fn read_binary_integer(
    bytes: &[u8],
    cursor: &mut usize,
    info: u8,
    object_end: usize,
) -> Result<i64, AirplayError> {
    if info > 3 {
        return Err(AirplayError::Invalid("binary plist integer"));
    }
    let width = 1usize << info;
    let end = cursor
        .checked_add(width)
        .ok_or(AirplayError::TooLarge("binary plist integer"))?;
    if end > object_end || end > bytes.len() {
        return Err(AirplayError::Invalid("binary plist integer"));
    }
    let raw = read_variable_u64(&bytes[*cursor..end])?;
    *cursor = end;
    let bits = width * 8;
    if bits == 64 {
        return Ok(raw as i64);
    }
    let sign = 1u64 << (bits - 1);
    if raw & sign == 0 {
        Ok(raw as i64)
    } else {
        Ok((raw | (!0u64 << bits)) as i64)
    }
}

fn read_uint(bytes: &[u8]) -> Result<usize, AirplayError> {
    usize::try_from(read_variable_u64(bytes)?)
        .map_err(|_| AirplayError::TooLarge("binary plist integer"))
}

fn read_variable_u64(bytes: &[u8]) -> Result<u64, AirplayError> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err(AirplayError::Invalid("binary plist integer"));
    }
    let mut value = 0u64;
    for byte in bytes {
        value = value
            .checked_shl(8)
            .and_then(|value| value.checked_add(u64::from(*byte)))
            .ok_or(AirplayError::TooLarge("binary plist integer"))?;
    }
    Ok(value)
}

fn read_u64(bytes: &[u8]) -> Result<u64, AirplayError> {
    if bytes.len() != 8 {
        return Err(AirplayError::Invalid("binary plist integer"));
    }
    Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| {
        AirplayError::Invalid("binary plist integer")
    })?))
}

/// Encodes one Property List value as deterministic XML bytes.
pub fn encode_xml_plist(value: &PlistValue) -> Result<Vec<u8>, AirplayError> {
    let mut entries = 0;
    validate_plist_value(value, 0, &mut entries)?;
    let mut output =
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\">".to_string();
    encode_plist_value(value, &mut output);
    output.push_str("</plist>");
    if output.len() > MAX_PLIST_BYTES {
        return Err(AirplayError::TooLarge("plist document"));
    }
    Ok(output.into_bytes())
}

struct PlistCursor<'a> {
    input: &'a str,
    position: usize,
    entries: usize,
}

impl<'a> PlistCursor<'a> {
    fn skip_whitespace(&mut self) {
        while let Some(character) = self.input[self.position..].chars().next() {
            if !character.is_whitespace() {
                break;
            }
            self.position += character.len_utf8();
        }
    }

    fn consume_open(&mut self, name: &str) -> Result<(), AirplayError> {
        self.skip_whitespace();
        let rest = &self.input[self.position..];
        let prefix = format!("<{name}");
        if !rest.starts_with(&prefix) {
            return Err(AirplayError::Invalid("plist opening tag"));
        }
        let after_name = rest.as_bytes().get(prefix.len()).copied();
        if !matches!(after_name, Some(b'>') | Some(b' ')) {
            return Err(AirplayError::Invalid("plist opening tag"));
        }
        let end = rest.find('>').ok_or(AirplayError::Invalid("plist tag"))?;
        if rest[..end].ends_with('/') {
            return Err(AirplayError::Invalid("plist self-closing tag"));
        }
        self.position += end + 1;
        Ok(())
    }

    fn consume_close(&mut self, name: &str) -> Result<(), AirplayError> {
        self.skip_whitespace();
        let close = format!("</{name}>");
        if !self.input[self.position..].starts_with(&close) {
            return Err(AirplayError::Invalid("plist closing tag"));
        }
        self.position += close.len();
        Ok(())
    }

    fn parse_value(&mut self, depth: usize) -> Result<PlistValue, AirplayError> {
        if depth > MAX_PLIST_DEPTH {
            return Err(AirplayError::TooLarge("plist nesting"));
        }
        self.skip_whitespace();
        let rest = &self.input[self.position..];
        if rest.starts_with("<dict") {
            self.consume_open("dict")?;
            let mut dictionary = BTreeMap::new();
            loop {
                self.skip_whitespace();
                if self.input[self.position..].starts_with("</dict>") {
                    self.consume_close("dict")?;
                    break;
                }
                let key = self.parse_text_tag("key")?;
                if dictionary.contains_key(&key) {
                    return Err(AirplayError::Invalid("plist duplicate key"));
                }
                let value = self.parse_value(depth + 1)?;
                dictionary.insert(key, value);
                self.entries = self.entries.saturating_add(1);
                if self.entries > MAX_PLIST_ENTRIES {
                    return Err(AirplayError::TooLarge("plist entries"));
                }
            }
            Ok(PlistValue::Dictionary(dictionary))
        } else if rest.starts_with("<array>") {
            self.consume_open("array")?;
            let mut values = Vec::new();
            loop {
                self.skip_whitespace();
                if self.input[self.position..].starts_with("</array>") {
                    self.consume_close("array")?;
                    break;
                }
                values.push(self.parse_value(depth + 1)?);
                self.entries = self.entries.saturating_add(1);
                if self.entries > MAX_PLIST_ENTRIES {
                    return Err(AirplayError::TooLarge("plist entries"));
                }
            }
            Ok(PlistValue::Array(values))
        } else if rest.starts_with("<string>") {
            Ok(PlistValue::String(self.parse_text_tag("string")?))
        } else if rest.starts_with("<integer>") {
            let value = self.parse_text_tag("integer")?;
            Ok(PlistValue::Integer(
                value
                    .parse::<i64>()
                    .map_err(|_| AirplayError::Invalid("plist integer"))?,
            ))
        } else if rest.starts_with("<data>") {
            let value = self.parse_text_tag("data")?;
            Ok(PlistValue::Data(decode_base64(&value)?))
        } else if rest.starts_with("<true/>") {
            self.position += "<true/>".len();
            Ok(PlistValue::Boolean(true))
        } else if rest.starts_with("<false/>") {
            self.position += "<false/>".len();
            Ok(PlistValue::Boolean(false))
        } else {
            Err(AirplayError::Invalid("plist value"))
        }
    }

    fn parse_text_tag(&mut self, name: &str) -> Result<String, AirplayError> {
        self.consume_open(name)?;
        let close = format!("</{name}>");
        let relative_end = self.input[self.position..]
            .find(&close)
            .ok_or(AirplayError::Invalid("plist text tag"))?;
        let raw = &self.input[self.position..self.position + relative_end];
        if raw.len() > MAX_PLIST_FIELD_BYTES {
            return Err(AirplayError::TooLarge("plist field"));
        }
        self.position += relative_end + close.len();
        let value = unescape_xml(raw)?;
        if value.len() > MAX_PLIST_FIELD_BYTES {
            return Err(AirplayError::TooLarge("plist field"));
        }
        Ok(value)
    }
}

fn validate_plist_value(
    value: &PlistValue,
    depth: usize,
    entries: &mut usize,
) -> Result<(), AirplayError> {
    if depth > MAX_PLIST_DEPTH {
        return Err(AirplayError::TooLarge("plist nesting"));
    }
    match value {
        PlistValue::String(value) => {
            if value.len() > MAX_PLIST_FIELD_BYTES {
                return Err(AirplayError::TooLarge("plist field"));
            }
        }
        PlistValue::Data(value) => {
            if value.len() > MAX_PLIST_FIELD_BYTES {
                return Err(AirplayError::TooLarge("plist data"));
            }
        }
        PlistValue::Array(values) => {
            if values.len() > MAX_PLIST_ENTRIES {
                return Err(AirplayError::TooLarge("plist entries"));
            }
            for value in values {
                *entries = entries.saturating_add(1);
                if *entries > MAX_PLIST_ENTRIES {
                    return Err(AirplayError::TooLarge("plist entries"));
                }
                validate_plist_value(value, depth + 1, entries)?;
            }
        }
        PlistValue::Dictionary(dictionary) => {
            if dictionary.len() > MAX_PLIST_ENTRIES {
                return Err(AirplayError::TooLarge("plist entries"));
            }
            for (key, value) in dictionary {
                if key.is_empty() || key.len() > MAX_PLIST_FIELD_BYTES {
                    return Err(AirplayError::InvalidField("plist key"));
                }
                *entries = entries.saturating_add(1);
                if *entries > MAX_PLIST_ENTRIES {
                    return Err(AirplayError::TooLarge("plist entries"));
                }
                validate_plist_value(value, depth + 1, entries)?;
            }
        }
        PlistValue::Integer(_) | PlistValue::Boolean(_) => {}
    }
    Ok(())
}

fn encode_plist_value(value: &PlistValue, output: &mut String) {
    match value {
        PlistValue::String(value) => {
            output.push_str("<string>");
            output.push_str(&escape_xml(value));
            output.push_str("</string>");
        }
        PlistValue::Integer(value) => output.push_str(&format!("<integer>{value}</integer>")),
        PlistValue::Boolean(value) => output.push_str(if *value { "<true/>" } else { "<false/>" }),
        PlistValue::Data(value) => {
            output.push_str("<data>");
            output.push_str(&encode_base64(value));
            output.push_str("</data>");
        }
        PlistValue::Array(values) => {
            output.push_str("<array>");
            for value in values {
                encode_plist_value(value, output);
            }
            output.push_str("</array>");
        }
        PlistValue::Dictionary(dictionary) => {
            output.push_str("<dict>");
            for (key, value) in dictionary {
                output.push_str("<key>");
                output.push_str(&escape_xml(key));
                output.push_str("</key>");
                encode_plist_value(value, output);
            }
            output.push_str("</dict>");
        }
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape_xml(value: &str) -> Result<String, AirplayError> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        let end = rest[index..]
            .find(';')
            .ok_or(AirplayError::Invalid("plist XML entity"))?;
        let entity = &rest[index..=index + end];
        output.push_str(match entity {
            "&amp;" => "&",
            "&lt;" => "<",
            "&gt;" => ">",
            "&quot;" => "\"",
            "&apos;" => "'",
            _ => return Err(AirplayError::Invalid("plist XML entity")),
        });
        rest = &rest[index + end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(bytes: &[u8]) -> String {
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(BASE64[(first >> 2) as usize] as char);
        output.push(BASE64[((first & 0x03) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            BASE64[((second & 0x0f) << 2 | third >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            BASE64[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn decode_base64(value: &str) -> Result<Vec<u8>, AirplayError> {
    let compact = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    if compact.len() % 4 != 0 || compact.len() / 4 * 3 > MAX_PLIST_FIELD_BYTES + 2 {
        return Err(AirplayError::Invalid("plist base64"));
    }
    let mut output = Vec::new();
    for chunk in compact.as_bytes().chunks(4) {
        let values = chunk
            .iter()
            .map(|byte| match byte {
                b'=' => Ok(64u8),
                _ => BASE64
                    .iter()
                    .position(|candidate| candidate == byte)
                    .map(|value| value as u8)
                    .ok_or(AirplayError::Invalid("plist base64")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 4 || values[0] >= 64 || values[1] >= 64 {
            return Err(AirplayError::Invalid("plist base64"));
        }
        output.push((values[0] << 2) | (values[1] >> 4));
        if values[2] < 64 {
            output.push((values[1] << 4) | (values[2] >> 2));
            if values[3] < 64 {
                output.push((values[2] << 6) | values[3]);
            } else if values[3] != 64 {
                return Err(AirplayError::Invalid("plist base64"));
            }
        } else if values[3] != 64 {
            return Err(AirplayError::Invalid("plist base64"));
        }
    }
    if output.len() > MAX_PLIST_FIELD_BYTES {
        return Err(AirplayError::TooLarge("plist data"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtsp_round_trip_preserves_cseq_and_body() {
        let mut headers = BTreeMap::new();
        headers.insert("cseq".to_string(), "4".to_string());
        headers.insert("content-length".to_string(), "3".to_string());
        let message = RtspMessage {
            start_line: RtspStartLine::Request {
                method: "ANNOUNCE".to_string(),
                uri: "rtsp://receiver/stream".to_string(),
            },
            headers,
            body: b"abc".to_vec(),
        };
        let decoded = RtspMessage::parse(&message.encode().unwrap()).unwrap();
        assert_eq!(decoded, message);
        assert_eq!(decoded.cseq().unwrap(), 4);
    }

    #[test]
    fn rtsp_rejects_duplicate_headers_and_bad_lengths() {
        let duplicate = b"RTSP/1.0 200 OK\r\nCSeq: 1\r\ncseq: 2\r\n\r\n";
        assert_eq!(
            RtspMessage::parse(duplicate),
            Err(AirplayError::Invalid("RTSP duplicate or oversized header"))
        );
        let bad_length = b"OPTIONS rtsp://x RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 2\r\n\r\nabc";
        assert_eq!(
            RtspMessage::parse(bad_length),
            Err(AirplayError::Invalid("RTSP content length"))
        );
    }

    #[test]
    fn rtp_round_trip_rejects_extension_and_unbounded_payload() {
        let packet = RtpAudioPacket {
            marker: true,
            payload_type: 96,
            sequence: 7,
            timestamp: 48_000,
            ssrc: 0x0102_0304,
            payload: vec![1, 2, 3],
        };
        assert_eq!(
            RtpAudioPacket::parse(&packet.encode().unwrap()).unwrap(),
            packet
        );
        let mut extension = packet.encode().unwrap();
        extension[0] |= 0x10;
        assert_eq!(
            RtpAudioPacket::parse(&extension),
            Err(AirplayError::Invalid("RTP padding or extension"))
        );
        assert!(
            RtpAudioPacket::encode(&RtpAudioPacket {
                payload: vec![0; MAX_RTP_PAYLOAD_BYTES + 1],
                ..packet
            })
            .is_err()
        );
    }

    #[test]
    fn rtp_receiver_socket_parses_loopback_datagrams_with_source() {
        let receiver = RtpReceiverSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let packet = rtp_packet(12);
        sender
            .send_to(&packet.encode().unwrap(), receiver.local_addr().unwrap())
            .unwrap();

        let (received, source) = receiver.recv().unwrap();
        assert_eq!(received, packet);
        assert_eq!(source, sender.local_addr().unwrap());
    }

    #[test]
    fn rtp_receiver_socket_preserves_bounded_parse_errors() {
        let receiver = RtpReceiverSocket::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(&[0_u8; 12], receiver.local_addr().unwrap())
            .unwrap();
        assert_eq!(receiver.recv(), Err(AirplayError::Invalid("RTP version")));

        let packet = rtp_packet(13);
        sender
            .send_to(&packet.encode().unwrap(), receiver.local_addr().unwrap())
            .unwrap();
        assert_eq!(receiver.recv().unwrap().0, packet);
    }

    fn rtp_packet(sequence: u16) -> RtpAudioPacket {
        RtpAudioPacket {
            marker: false,
            payload_type: 96,
            sequence,
            timestamp: u32::from(sequence) * 960,
            ssrc: 7,
            payload: vec![sequence as u8],
        }
    }

    #[test]
    fn rtp_jitter_buffer_reorders_and_recovers_gaps() {
        let mut buffer = RtpJitterBuffer::new(4).unwrap();
        assert_eq!(
            buffer.push(rtp_packet(10)).unwrap(),
            RtpPushResult::Buffered
        );
        assert_eq!(
            buffer.push(rtp_packet(12)).unwrap(),
            RtpPushResult::Buffered
        );
        assert_eq!(
            buffer.push(rtp_packet(11)).unwrap(),
            RtpPushResult::Buffered
        );
        assert_eq!(buffer.pop_ready().unwrap().sequence, 10);
        assert_eq!(buffer.pop_ready().unwrap().sequence, 11);
        assert_eq!(buffer.pop_ready().unwrap().sequence, 12);
        assert_eq!(buffer.buffered_len(), 0);

        let mut recovery = RtpJitterBuffer::new(2).unwrap();
        recovery.push(rtp_packet(10)).unwrap();
        recovery.push(rtp_packet(12)).unwrap();
        assert_eq!(recovery.pop_ready().unwrap().sequence, 10);
        assert!(recovery.pop_ready().is_none());
        assert_eq!(recovery.skip_missing_to(12).unwrap(), 1);
        assert_eq!(recovery.pop_ready().unwrap().sequence, 12);
    }

    #[test]
    fn rtp_jitter_buffer_bounds_duplicates_late_packets_and_wrap() {
        let mut buffer = RtpJitterBuffer::new(1).unwrap();
        assert!(RtpJitterBuffer::new(0).is_err());
        assert!(RtpJitterBuffer::new(MAX_RTP_JITTER_PACKETS + 1).is_err());
        buffer.push(rtp_packet(u16::MAX)).unwrap();
        assert_eq!(buffer.pop_ready().unwrap().sequence, u16::MAX);
        assert_eq!(
            buffer.push(rtp_packet(u16::MAX)).unwrap(),
            RtpPushResult::DroppedLate
        );
        assert_eq!(buffer.dropped_late(), 1);
        assert_eq!(buffer.push(rtp_packet(0)).unwrap(), RtpPushResult::Buffered);
        assert_eq!(
            buffer.push(rtp_packet(0)),
            Err(AirplayError::Invalid("RTP duplicate sequence"))
        );
        assert_eq!(buffer.pop_ready().unwrap().sequence, 0);
        assert!(buffer.skip_missing_to(u16::MAX).is_err());
    }

    #[test]
    fn mirror_audio_pipeline_reorders_rtp_and_builds_access_units() {
        let format = MirrorAudioFormat::new(MirrorAudioCodec::Aac, 48_000, 2).unwrap();
        let mut pipeline = MirrorAudioPipeline::new(format, 96, 1_024, 4).unwrap();
        let packet = |sequence: u16, timestamp: u32, payload: u8| RtpAudioPacket {
            marker: false,
            payload_type: 96,
            sequence,
            timestamp,
            ssrc: 7,
            payload: vec![payload],
        };
        pipeline.push(packet(1, 0, 1)).unwrap();
        pipeline.push(packet(3, 2_048, 3)).unwrap();
        pipeline.push(packet(2, 1_024, 2)).unwrap();
        let first = pipeline.pop_ready().unwrap().unwrap();
        assert_eq!(first.pts_samples, 0);
        assert_eq!(first.duration_samples, 1_024);
        assert_eq!(first.payload, vec![1]);
        let second = pipeline.pop_ready().unwrap().unwrap();
        assert_eq!(second.pts_samples, 1_024);
        assert_eq!(second.payload, vec![2]);
        let third = pipeline.pop_ready().unwrap().unwrap();
        assert_eq!(third.pts_samples, 2_048);
        assert_eq!(third.payload, vec![3]);
        assert!(pipeline.pop_ready().unwrap().is_none());
    }

    #[test]
    fn mirror_audio_pipeline_enforces_payload_and_loss_policy() {
        let pcm = MirrorAudioFormat::new(MirrorAudioCodec::Pcm16, 48_000, 2).unwrap();
        let mut pipeline = MirrorAudioPipeline::new(pcm, 97, 960, 2).unwrap();
        let bad_payload = RtpAudioPacket {
            marker: false,
            payload_type: 97,
            sequence: 4,
            timestamp: 3_840,
            ssrc: 7,
            payload: vec![1],
        };
        assert_eq!(
            pipeline.push(bad_payload),
            Err(AirplayError::Invalid("mirror PCM payload"))
        );
        let wrong_type = RtpAudioPacket {
            payload_type: 96,
            payload: vec![0; 4],
            ..rtp_packet(4)
        };
        assert_eq!(
            pipeline.push(wrong_type),
            Err(AirplayError::Invalid("mirror audio payload type"))
        );
        pipeline
            .push(RtpAudioPacket {
                payload_type: 97,
                payload: vec![0; 4],
                ..rtp_packet(10)
            })
            .unwrap();
        pipeline
            .push(RtpAudioPacket {
                payload_type: 97,
                payload: vec![0; 4],
                ..rtp_packet(12)
            })
            .unwrap();
        assert!(pipeline.pop_ready().unwrap().is_some());
        assert_eq!(pipeline.skip_missing_to(12).unwrap(), 1);
        assert!(pipeline.pop_ready().unwrap().is_some());
    }

    #[test]
    fn mirror_video_pipeline_reorders_stap_a_and_preserves_orientation() {
        let mut pipeline = MirrorVideoPipeline::new(MirrorOrientation::Deg90, 98, 8).unwrap();
        let packet = |sequence: u16, marker: bool, payload: Vec<u8>| RtpVideoPacket {
            marker,
            payload_type: 98,
            sequence,
            timestamp: 90_000,
            ssrc: 11,
            payload,
        };
        pipeline.push(packet(1, false, vec![0x67, 0x01])).unwrap();
        pipeline
            .push(packet(
                3,
                true,
                vec![0x78, 0, 2, 0x68, 0x02, 0, 2, 0x65, 0x03],
            ))
            .unwrap();
        pipeline.push(packet(2, false, vec![0x61, 0x04])).unwrap();

        let unit = pipeline.pop_ready().unwrap().unwrap();
        assert_eq!(unit.pts_90khz, 90_000);
        assert_eq!(unit.orientation, MirrorOrientation::Deg90);
        assert!(unit.keyframe);
        assert_eq!(unit.nal_units.len(), 4);
        assert_eq!(unit.to_annex_b().unwrap().len(), 4 * 4 + 2 + 2 + 2 + 2);
        assert!(pipeline.pop_ready().unwrap().is_none());
    }

    #[test]
    fn mirror_video_pipeline_reassembles_fu_a_and_updates_orientation() {
        let mut pipeline = MirrorVideoPipeline::new(MirrorOrientation::Deg0, 98, 8).unwrap();
        let packet = |sequence: u16, marker: bool, payload: Vec<u8>| RtpVideoPacket {
            marker,
            payload_type: 98,
            sequence,
            timestamp: 180_000,
            ssrc: 12,
            payload,
        };
        pipeline
            .push(packet(10, false, vec![0x7c, 0x85, 0x11, 0x22]))
            .unwrap();
        pipeline
            .push(packet(12, true, vec![0x7c, 0x45, 0x44]))
            .unwrap();
        pipeline
            .push(packet(11, false, vec![0x7c, 0x05, 0x33]))
            .unwrap();
        pipeline.set_orientation(MirrorOrientation::Deg180);

        let unit = pipeline.pop_ready().unwrap().unwrap();
        assert_eq!(unit.orientation, MirrorOrientation::Deg180);
        assert!(unit.keyframe);
        assert_eq!(unit.nal_units[0].bytes, vec![0x65, 0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn mirror_video_pipeline_rejects_bad_payload_and_recovers_after_loss() {
        let mut pipeline = MirrorVideoPipeline::new(MirrorOrientation::Deg0, 98, 4).unwrap();
        assert_eq!(
            pipeline.push(RtpVideoPacket {
                payload_type: 97,
                payload: vec![0x61, 1],
                ..rtp_packet(1)
            }),
            Err(AirplayError::Invalid("mirror video payload type"))
        );

        let fu_start = RtpVideoPacket {
            marker: false,
            payload_type: 98,
            timestamp: 270_000,
            payload: vec![0x7c, 0x85, 0x01],
            ..rtp_packet(20)
        };
        let fu_end = RtpVideoPacket {
            marker: true,
            payload_type: 98,
            timestamp: 270_000,
            payload: vec![0x7c, 0x45, 0x02],
            ..rtp_packet(22)
        };
        pipeline.push(fu_start).unwrap();
        pipeline.push(fu_end).unwrap();
        assert!(pipeline.pop_ready().unwrap().is_none());
        assert_eq!(pipeline.skip_missing_to(23).unwrap(), 2);
        pipeline
            .push(RtpVideoPacket {
                marker: true,
                payload_type: 98,
                timestamp: 273_000,
                payload: vec![0x65, 0x03],
                ..rtp_packet(23)
            })
            .unwrap();
        let recovered = pipeline.pop_ready().unwrap().unwrap();
        assert!(recovered.keyframe);
        assert_eq!(recovered.nal_units[0].bytes, vec![0x65, 0x03]);
        assert!(pipeline.pop_ready().unwrap().is_none());
    }

    fn request(method: &str, cseq: u32, headers: &[(&str, &str)], body: &[u8]) -> RtspMessage {
        let mut all = BTreeMap::new();
        all.insert("cseq".to_string(), cseq.to_string());
        for (name, value) in headers {
            all.insert((*name).to_string(), (*value).to_string());
        }
        RtspMessage {
            start_line: RtspStartLine::Request {
                method: method.to_string(),
                uri: "rtsp://receiver/stream".to_string(),
            },
            headers: all,
            body: body.to_vec(),
        }
    }

    fn announce() -> RtspMessage {
        request(
            "ANNOUNCE",
            2,
            &[],
            b"v=0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 AppleLossless/44100/2\r\n",
        )
    }

    #[test]
    fn sdp_and_raop_session_complete_bounded_negotiation() {
        let format = RaopAudioFormat::from_sdp(&announce().body).unwrap();
        assert_eq!(format.codec, RaopCodec::Alac);
        assert_eq!(format.sample_rate, 44_100);
        assert_eq!(format.channels, 2);
        assert_eq!(format.payload_type, 96);

        let mut session = RaopSession::new(6000).unwrap();
        assert_eq!(session.state(), RaopSessionState::Idle);
        assert_eq!(
            session
                .handle(&request("OPTIONS", 1, &[], &[]))
                .unwrap()
                .cseq()
                .unwrap(),
            1
        );
        session.handle(&announce()).unwrap();
        assert_eq!(session.state(), RaopSessionState::Announced);
        let setup = request(
            "SETUP",
            3,
            &[(
                "transport",
                "RTP/AVP/UDP;unicast;mode=record;control_port=6001",
            )],
            &[],
        );
        let response = session.handle(&setup).unwrap();
        assert_eq!(session.state(), RaopSessionState::Setup);
        assert_eq!(
            response.headers.get("transport"),
            Some(&"RTP/AVP/UDP;unicast;mode=record;server_port=6000".to_string())
        );
        session.handle(&request("RECORD", 4, &[], &[])).unwrap();
        assert_eq!(session.state(), RaopSessionState::Recording);
        session.handle(&request("FLUSH", 5, &[], &[])).unwrap();
        session.handle(&request("TEARDOWN", 6, &[], &[])).unwrap();
        assert_eq!(session.state(), RaopSessionState::Closed);
        assert!(session.audio_format().is_none());
    }

    #[test]
    fn raop_rejects_wrong_codec_sdp_transport_and_state_order() {
        let unsupported = b"v=0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 OPUS/48000/2\r\n";
        assert_eq!(
            RaopAudioFormat::from_sdp(unsupported),
            Err(AirplayError::Invalid("SDP codec"))
        );
        let mut session = RaopSession::new(6000).unwrap();
        assert_eq!(
            session.handle(&request("RECORD", 1, &[], &[])),
            Err(AirplayError::Invalid("RAOP session transition"))
        );
        session.handle(&announce()).unwrap();
        let bad_setup = request(
            "SETUP",
            3,
            &[("transport", "RTP/AVP/TCP;interleaved=0-1")],
            &[],
        );
        assert_eq!(
            session.handle(&bad_setup),
            Err(AirplayError::Invalid("RAOP transport"))
        );
        assert_eq!(session.state(), RaopSessionState::Announced);
    }

    #[test]
    fn xml_plist_round_trip_preserves_bounded_values_and_escaping() {
        let mut dictionary = BTreeMap::new();
        dictionary.insert(
            "name".to_string(),
            PlistValue::String("Frame & Ark".to_string()),
        );
        dictionary.insert("count".to_string(), PlistValue::Integer(42));
        dictionary.insert("enabled".to_string(), PlistValue::Boolean(true));
        dictionary.insert("token".to_string(), PlistValue::Data(vec![0, 1, 2, 255]));
        dictionary.insert(
            "items".to_string(),
            PlistValue::Array(vec![
                PlistValue::String("one".to_string()),
                PlistValue::Boolean(false),
            ]),
        );
        let value = PlistValue::Dictionary(dictionary);
        let encoded = encode_xml_plist(&value).unwrap();
        assert!(
            std::str::from_utf8(&encoded)
                .unwrap()
                .contains("Frame &amp; Ark")
        );
        assert_eq!(parse_xml_plist(&encoded).unwrap(), value);
    }

    #[test]
    fn xml_plist_rejects_duplicate_keys_bad_entities_and_deep_nesting() {
        let duplicate = b"<plist><dict><key>a</key><string>1</string><key>a</key><string>2</string></dict></plist>";
        assert_eq!(
            parse_xml_plist(duplicate),
            Err(AirplayError::Invalid("plist duplicate key"))
        );
        let bad_entity = b"<plist><string>&unknown;</string></plist>";
        assert_eq!(
            parse_xml_plist(bad_entity),
            Err(AirplayError::Invalid("plist XML entity"))
        );
        let mut nested = "<plist>".to_string();
        for _ in 0..=MAX_PLIST_DEPTH {
            nested.push_str("<array>");
        }
        nested.push_str("<string>x</string>");
        for _ in 0..=MAX_PLIST_DEPTH {
            nested.push_str("</array>");
        }
        nested.push_str("</plist>");
        assert_eq!(
            parse_xml_plist(nested.as_bytes()),
            Err(AirplayError::TooLarge("plist nesting"))
        );
    }

    #[test]
    fn binary_plist_round_trip_parses_dictionary_and_utf16() {
        let mut bytes = b"bplist00".to_vec();
        // Object 0: {"name": "FrameArk", "count": 42}.
        bytes.extend_from_slice(&[0xd2, 1, 2, 3, 4]);
        bytes.extend_from_slice(&[0x54, b'n', b'a', b'm', b'e']);
        bytes.extend_from_slice(&[0x55, b'c', b'o', b'u', b'n', b't']);
        bytes.extend_from_slice(&[0x58, b'F', b'r', b'a', b'm', b'e', b'A', b'r', b'k']);
        bytes.extend_from_slice(&[0x10, 42]);
        let offset_table = bytes.len();
        bytes.extend_from_slice(&[8, 13, 18, 24, 33]);
        bytes.extend_from_slice(&[0; 6]);
        bytes.extend_from_slice(&[1, 1]); // offset-int and object-ref sizes
        bytes.extend_from_slice(&5u64.to_be_bytes());
        bytes.extend_from_slice(&0u64.to_be_bytes());
        bytes.extend_from_slice(&(offset_table as u64).to_be_bytes());

        let mut expected = BTreeMap::new();
        expected.insert(
            "name".to_string(),
            PlistValue::String("FrameArk".to_string()),
        );
        expected.insert("count".to_string(), PlistValue::Integer(42));
        assert_eq!(
            parse_binary_plist(&bytes).unwrap(),
            PlistValue::Dictionary(expected)
        );

        let mut utf16 = b"bplist00".to_vec();
        utf16.extend_from_slice(&[0x62, 0x00, 0x46, 0x00, 0x41]);
        let utf16_offset_table = utf16.len();
        utf16.extend_from_slice(&[8]);
        utf16.extend_from_slice(&[0; 6]);
        utf16.extend_from_slice(&[1, 1]);
        utf16.extend_from_slice(&1u64.to_be_bytes());
        utf16.extend_from_slice(&0u64.to_be_bytes());
        utf16.extend_from_slice(&(utf16_offset_table as u64).to_be_bytes());
        assert_eq!(
            parse_binary_plist(&utf16).unwrap(),
            PlistValue::String("FA".to_string())
        );
    }

    #[test]
    fn binary_plist_rejects_cycles_unsupported_objects_and_bad_bounds() {
        let mut cycle = b"bplist00".to_vec();
        cycle.extend_from_slice(&[0xa1, 0]);
        let offset_table = cycle.len();
        cycle.extend_from_slice(&[8]);
        cycle.extend_from_slice(&[0; 6]);
        cycle.extend_from_slice(&[1, 1]);
        cycle.extend_from_slice(&1u64.to_be_bytes());
        cycle.extend_from_slice(&0u64.to_be_bytes());
        cycle.extend_from_slice(&(offset_table as u64).to_be_bytes());
        assert_eq!(
            parse_binary_plist(&cycle),
            Err(AirplayError::Invalid("binary plist cycle"))
        );

        let mut real = b"bplist00".to_vec();
        real.extend_from_slice(&[0x23, 0, 0, 0]);
        let real_offset_table = real.len();
        real.extend_from_slice(&[8]);
        real.extend_from_slice(&[0; 6]);
        real.extend_from_slice(&[1, 1]);
        real.extend_from_slice(&1u64.to_be_bytes());
        real.extend_from_slice(&0u64.to_be_bytes());
        real.extend_from_slice(&(real_offset_table as u64).to_be_bytes());
        assert_eq!(
            parse_binary_plist(&real),
            Err(AirplayError::Invalid("binary plist object type"))
        );

        let mut malformed = real.clone();
        let trailer = malformed.len() - 32;
        malformed[trailer + 24..trailer + 32].copy_from_slice(&13u64.to_be_bytes());
        assert_eq!(
            parse_binary_plist(&malformed),
            Err(AirplayError::Invalid("binary plist offset table"))
        );
    }

    #[test]
    fn mirror_video_contract_parses_annex_b_and_avcc_keyframes() {
        let annex_b = [0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 1, 0x65, 4, 5];
        let unit = MirrorVideoAccessUnit::from_annex_b(90_000, MirrorOrientation::Deg90, &annex_b)
            .unwrap();
        assert!(unit.keyframe);
        assert_eq!(unit.nal_units.len(), 2);
        assert_eq!(
            unit.to_annex_b().unwrap(),
            [0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x65, 4, 5]
        );

        let avcc = [0, 0, 0, 2, 0x67, 1, 0, 0, 0, 2, 0x65, 2];
        let decoded =
            MirrorVideoAccessUnit::from_avcc(90_001, MirrorOrientation::Deg0, &avcc).unwrap();
        assert!(decoded.keyframe);
        assert_eq!(decoded.nal_units[1].nal_type, 5);
        assert_eq!(
            MirrorOrientation::try_from(180).unwrap(),
            MirrorOrientation::Deg180
        );
        assert!(MirrorOrientation::try_from(45).is_err());
    }

    #[test]
    fn mirror_contract_rejects_bad_nals_and_bounds_av_sync() {
        assert!(
            MirrorVideoAccessUnit::from_annex_b(0, MirrorOrientation::Deg0, &[0, 0, 1, 0x80])
                .is_err()
        );
        assert!(
            MirrorVideoAccessUnit::from_avcc(0, MirrorOrientation::Deg0, &[0, 0, 0, 5, 1]).is_err()
        );
        let clock = MirrorClock::new(48_000).unwrap();
        assert_eq!(clock.audio_samples_to_video_ticks(48_000).unwrap(), 90_000);
        assert_eq!(clock.offset_90khz(90_000, 48_000).unwrap(), 0);
        assert!(clock.offset_90khz(90_000 + 90_001, 48_000).is_err());
        assert!(MirrorClock::new(1_000).is_err());
    }

    #[test]
    fn mirror_audio_contract_bounds_aac_pcm_and_timestamps() {
        let aac = MirrorAudioFormat::new(MirrorAudioCodec::Aac, 48_000, 2).unwrap();
        let aac_unit = MirrorAudioAccessUnit::new(96_000, 1_024, aac, vec![1, 2, 3]).unwrap();
        assert_eq!(aac_unit.end_samples().unwrap(), 97_024);

        let pcm = MirrorAudioFormat::new(MirrorAudioCodec::Pcm16, 44_100, 2).unwrap();
        let pcm_unit = MirrorAudioAccessUnit::new(0, 441, pcm, vec![0; 1_764]).unwrap();
        assert_eq!(pcm_unit.payload.len(), 1_764);
        assert!(MirrorAudioFormat::new(MirrorAudioCodec::Aac, 7_999, 2).is_err());
        assert!(MirrorAudioAccessUnit::new(0, 0, aac, vec![1]).is_err());
        assert!(MirrorAudioAccessUnit::new(0, 1, pcm, vec![1]).is_err());
        assert!(
            MirrorAudioAccessUnit::new(u64::MAX, 1, aac, vec![1])
                .unwrap()
                .end_samples()
                .is_err()
        );
    }

    #[test]
    fn mirror_session_negotiates_config_transport_and_lifecycle() {
        let mut config = BTreeMap::new();
        config.insert("width".to_string(), PlistValue::Integer(1920));
        config.insert("height".to_string(), PlistValue::Integer(1080));
        config.insert("orientation".to_string(), PlistValue::Integer(90));
        config.insert("audio_sample_rate".to_string(), PlistValue::Integer(48_000));
        let body = encode_xml_plist(&PlistValue::Dictionary(config)).unwrap();
        let mut session = MirrorSession::new("mirror-session", 7010).unwrap();
        assert_eq!(session.state(), MirrorSessionState::Idle);
        session.handle(&request("OPTIONS", 1, &[], &[])).unwrap();
        let setup = request(
            "SETUP",
            2,
            &[(
                "transport",
                "RTP/AVP/UDP;unicast;mode=record;control_port=7011;timing_port=7012",
            )],
            &body,
        );
        let setup_response = session.handle(&setup).unwrap();
        assert_eq!(session.state(), MirrorSessionState::Setup);
        assert_eq!(session.transport().unwrap().control_port, 7011);
        assert_eq!(session.transport().unwrap().timing_port, Some(7012));
        assert_eq!(session.config().unwrap().width, 1920);
        assert_eq!(
            setup_response.headers.get("transport"),
            Some(&"RTP/AVP/UDP;unicast;mode=record;server_port=7010;control_port=7011".to_string())
        );
        session.handle(&request("RECORD", 3, &[], &[])).unwrap();
        assert_eq!(session.state(), MirrorSessionState::Streaming);
        let parameter = session
            .handle(&request("GET_PARAMETER", 4, &[], b"volume\r\n"))
            .unwrap();
        assert_eq!(parameter.body, b"volume\r\n");
        session.handle(&request("FLUSH", 5, &[], &[])).unwrap();
        session.handle(&request("TEARDOWN", 6, &[], &[])).unwrap();
        assert_eq!(session.state(), MirrorSessionState::Closed);
    }

    #[test]
    fn mirror_session_rejects_invalid_config_transport_and_order() {
        let mut session = MirrorSession::new("mirror-session", 7010).unwrap();
        assert_eq!(
            session.handle(&request("RECORD", 1, &[], &[])),
            Err(AirplayError::Invalid("mirror RTSP transition"))
        );
        let mut config = BTreeMap::new();
        config.insert("width".to_string(), PlistValue::Integer(0));
        config.insert("height".to_string(), PlistValue::Integer(1080));
        config.insert("orientation".to_string(), PlistValue::Integer(0));
        config.insert("audio_sample_rate".to_string(), PlistValue::Integer(48_000));
        let body = encode_xml_plist(&PlistValue::Dictionary(config)).unwrap();
        assert_eq!(
            session.handle(&request(
                "SETUP",
                2,
                &[("transport", "RTP/AVP/TCP;interleaved=0-1")],
                &body,
            )),
            Err(AirplayError::Invalid("mirror transport"))
        );
        assert!(MirrorSession::new("", 7010).is_err());
        assert!(MirrorSession::new("mirror-session", 0).is_err());
    }
}
