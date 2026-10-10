//! Bounded AirPlay/RAOP protocol contracts.
//!
//! This M4 foundation parses RTSP control messages and unencrypted RTP audio
//! packets. It intentionally does not implement Apple pairing, FairPlay,
//! AES-CTR audio decryption, ALAC/AAC decoding, or device interoperability.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// Maximum RTSP message size accepted before parsing.
pub const MAX_RTSP_BYTES: usize = 64 * 1024;
/// Maximum RTSP body size accepted before parsing.
pub const MAX_RTSP_BODY_BYTES: usize = 32 * 1024;
/// Maximum RTP payload size accepted by the bounded audio contract.
pub const MAX_RTP_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum SDP body accepted by ANNOUNCE.
pub const MAX_SDP_BYTES: usize = 16 * 1024;
/// Maximum SDP attribute lines accepted by one announcement.
pub const MAX_SDP_LINES: usize = 64;
/// Maximum RAOP session identifier length.
pub const MAX_SESSION_ID_BYTES: usize = 128;

/// Errors raised by bounded RTSP/RTP parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AirplayError {
    /// Input exceeded a protocol bound.
    TooLarge(&'static str),
    /// Input violated the protocol grammar.
    Invalid(&'static str),
    /// A caller supplied an invalid session field.
    InvalidField(&'static str),
}

impl Display for AirplayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(field) => write!(formatter, "AirPlay {field} exceeds its bound"),
            Self::Invalid(field) => write!(formatter, "invalid AirPlay {field}"),
            Self::InvalidField(field) => write!(formatter, "invalid AirPlay field {field}"),
        }
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
}
