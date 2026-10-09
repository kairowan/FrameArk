//! Experimental v1 control payload schema. All integers are big-endian.
use crate::SessionOffer;
use frameark_core::{AudioConfig, FrameArkError, MediaCodec, Result, SessionState, VideoConfig};
use frameark_transport::ControlMessage;

/// Transport envelope type for native control requests.
pub const REQUEST: u8 = 16;
/// Transport envelope type for native control responses.
pub const RESPONSE: u8 = 17;
const VERSION: u8 = 1;

/// Sequenced native control request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    /// Monotonically increasing ID, starting at one on each connection.
    pub id: u32,
    /// The requested operation.
    pub command: Command,
}

/// Experimental native control commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    /// Propose exact media configuration before backend preparation.
    Offer(SessionOffer),
    /// Start an already prepared backend.
    Start,
    /// Stop and release the session.
    Stop,
    /// Query the shared session state.
    Status,
}

/// Wire-safe result without backend messages or peer data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// Operation accepted.
    Ok,
    /// Malformed request or sequence.
    Invalid,
    /// Proposal not supported.
    Unsupported,
    /// Operation not valid in current state.
    InvalidState,
    /// Local backend failed.
    Backend,
}

/// Validated response to one control request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Response {
    /// Echoed request ID.
    pub id: u32,
    /// Operation outcome.
    pub status: Status,
    /// Shared session state after applying the operation.
    pub state: SessionState,
    /// Exact selected configuration, only on a successful offer.
    pub offer: Option<SessionOffer>,
}

fn malformed() -> FrameArkError {
    FrameArkError::invalid_argument("fanp.control_payload", "invalid native control payload")
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.0.len() < N {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(N);
        self.0 = rest;
        head.try_into().map_err(|_| malformed())
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    fn finish(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(malformed())
        }
    }
}

fn offer_bytes(offer: SessionOffer, out: &mut Vec<u8>) -> Result<()> {
    offer.validate()?;
    out.push(u8::from(offer.video.is_some()) | (u8::from(offer.audio.is_some()) << 1));
    out.extend(offer.latency_ms.to_be_bytes());
    if let Some(v) = offer.video {
        out.push(1); // H.264
        for value in [
            v.width,
            v.height,
            v.frame_rate_numerator,
            v.frame_rate_denominator,
        ] {
            out.extend(value.to_be_bytes());
        }
    }
    if let Some(a) = offer.audio {
        out.push(if a.codec == MediaCodec::Opus { 2 } else { 3 });
        out.extend(a.sample_rate.to_be_bytes());
        out.extend(a.channels.to_be_bytes());
    }
    Ok(())
}

fn parse_offer(reader: &mut Reader<'_>) -> Result<SessionOffer> {
    let flags = reader.byte()?;
    if flags == 0 || flags & !3 != 0 {
        return Err(malformed());
    }
    let latency_ms = reader.u16()?;
    let video = if flags & 1 != 0 {
        if reader.byte()? != 1 {
            return Err(malformed());
        }
        Some(VideoConfig {
            codec: MediaCodec::H264,
            width: reader.u32()?,
            height: reader.u32()?,
            frame_rate_numerator: reader.u32()?,
            frame_rate_denominator: reader.u32()?,
        })
    } else {
        None
    };
    let audio = if flags & 2 != 0 {
        let codec = match reader.byte()? {
            2 => MediaCodec::Opus,
            3 => MediaCodec::Aac,
            _ => return Err(malformed()),
        };
        Some(AudioConfig {
            codec,
            sample_rate: reader.u32()?,
            channels: reader.u16()?,
        })
    } else {
        None
    };
    let offer = SessionOffer {
        video,
        audio,
        latency_ms,
    };
    offer.validate()?;
    Ok(offer)
}

impl Request {
    /// Encodes a validated request into the bounded transport envelope.
    pub fn encode(self) -> Result<ControlMessage> {
        if self.id == 0 {
            return Err(malformed());
        }
        let mut bytes = vec![VERSION];
        bytes.extend(self.id.to_be_bytes());
        bytes.push(match self.command {
            Command::Offer(_) => 1,
            Command::Start => 2,
            Command::Stop => 3,
            Command::Status => 4,
        });
        if let Command::Offer(offer) = self.command {
            offer_bytes(offer, &mut bytes)?;
        }
        ControlMessage::new(REQUEST, &bytes).map_err(|_| malformed())
    }
    /// Decodes a payload without allocating untrusted-length collections.
    pub fn decode(message: &ControlMessage) -> Result<Self> {
        if message.kind() != REQUEST {
            return Err(malformed());
        }
        let mut reader = Reader(message.payload());
        if reader.byte()? != VERSION {
            return Err(malformed());
        }
        let id = reader.u32()?;
        if id == 0 {
            return Err(malformed());
        }
        let command = match reader.byte()? {
            1 => Command::Offer(parse_offer(&mut reader)?),
            2 => Command::Start,
            3 => Command::Stop,
            4 => Command::Status,
            _ => return Err(malformed()),
        };
        reader.finish()?;
        Ok(Self { id, command })
    }
}

impl Response {
    /// Encodes a response without diagnostics or private peer data.
    pub fn encode(self) -> Result<ControlMessage> {
        if self.id == 0 || (self.offer.is_some() && self.status != Status::Ok) {
            return Err(malformed());
        }
        let mut bytes = vec![VERSION];
        bytes.extend(self.id.to_be_bytes());
        bytes.push(match self.status {
            Status::Ok => 0,
            Status::Invalid => 1,
            Status::Unsupported => 2,
            Status::InvalidState => 3,
            Status::Backend => 4,
        });
        bytes.push(state_code(self.state));
        if let Some(offer) = self.offer {
            offer_bytes(offer, &mut bytes)?;
        }
        ControlMessage::new(RESPONSE, &bytes).map_err(|_| malformed())
    }
    /// Decodes a bounded response, rejecting extra and unknown fields.
    pub fn decode(message: &ControlMessage) -> Result<Self> {
        if message.kind() != RESPONSE {
            return Err(malformed());
        }
        let mut reader = Reader(message.payload());
        if reader.byte()? != VERSION {
            return Err(malformed());
        }
        let id = reader.u32()?;
        if id == 0 {
            return Err(malformed());
        }
        let status = match reader.byte()? {
            0 => Status::Ok,
            1 => Status::Invalid,
            2 => Status::Unsupported,
            3 => Status::InvalidState,
            4 => Status::Backend,
            _ => return Err(malformed()),
        };
        let state = match reader.byte()? {
            0 => SessionState::Negotiating,
            1 => SessionState::Preparing,
            2 => SessionState::Streaming,
            3 => SessionState::Closed,
            _ => return Err(malformed()),
        };
        let offer = if reader.0.is_empty() {
            None
        } else {
            Some(parse_offer(&mut reader)?)
        };
        reader.finish()?;
        if offer.is_some() && status != Status::Ok {
            return Err(malformed());
        }
        Ok(Self {
            id,
            status,
            state,
            offer,
        })
    }
}

fn state_code(state: SessionState) -> u8 {
    match state {
        SessionState::Negotiating => 0,
        SessionState::Preparing => 1,
        SessionState::Streaming => 2,
        SessionState::Closed => 3,
        _ => 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frameark_core::{MediaCodec, VideoConfig};

    fn offer() -> SessionOffer {
        SessionOffer {
            video: Some(VideoConfig {
                codec: MediaCodec::H264,
                width: 1280,
                height: 720,
                frame_rate_numerator: 30,
                frame_rate_denominator: 1,
            }),
            audio: None,
            latency_ms: 120,
        }
    }

    #[test]
    fn request_and_response_round_trip() {
        let request = Request {
            id: 7,
            command: Command::Offer(offer()),
        };
        let decoded = Request::decode(&request.encode().unwrap()).unwrap();
        assert_eq!(decoded, request);
        let response = Response {
            id: 7,
            status: Status::Ok,
            state: SessionState::Preparing,
            offer: Some(offer()),
        };
        let decoded = Response::decode(&response.encode().unwrap()).unwrap();
        assert_eq!(decoded, response);
    }

    #[test]
    fn malformed_trailing_and_wrong_message_are_rejected() {
        let message = ControlMessage::new(REQUEST, &[VERSION, 0, 0, 0, 1, 4, 0]).unwrap();
        assert!(Request::decode(&message).is_err());
        let wrong = ControlMessage::new(RESPONSE, &[VERSION, 0, 0, 0, 1, 4]).unwrap();
        assert!(Request::decode(&wrong).is_err());
    }
}
