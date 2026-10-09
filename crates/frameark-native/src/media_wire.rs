//! Bounded FANP v1 media-frame wire representation.
//!
//! The control plane negotiates codec and track configuration separately. This
//! module only serializes timestamped encoded access units after that
//! negotiation. It rejects malformed or trailing bytes before allocating an
//! untrusted payload and applies the same per-kind bounds as `frameark-media`.

use frameark_core::{FrameArkError, Result, TimeBase, TrackId};
use frameark_media::{
    AudioPacket, MAX_AUDIO_PAYLOAD_BYTES, MAX_VIDEO_PAYLOAD_BYTES, MediaPacket, MediaTimestamp,
    VideoFrame,
};

const MAGIC: [u8; 4] = *b"FAM1";
const VERSION: u8 = 1;
const VIDEO: u8 = 1;
const AUDIO: u8 = 2;
const KEYFRAME: u8 = 1;
const HAS_DECODE_TIMESTAMP: u8 = 2;
const NO_DECODE_TIMESTAMP: i64 = i64::MIN;
const MAX_TRACK_ID_BYTES: usize = 128;
const HEADER_BYTES: usize = 50;

fn malformed() -> FrameArkError {
    FrameArkError::invalid_argument("fanp.media_frame", "invalid native media frame")
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.0.len() < N {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(N);
        self.0 = rest;
        head.try_into().map_err(|_| malformed())
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take()?))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take()?))
    }

    fn bytes(&mut self, length: usize) -> Result<Vec<u8>> {
        if self.0.len() < length {
            return Err(malformed());
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(head.to_vec())
    }

    fn finish(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(malformed())
        }
    }
}

struct Header<'a> {
    kind: u8,
    flags: u8,
    sequence: u64,
    timestamp: MediaTimestamp,
    decode: i64,
    duration_ticks: u32,
    track_id: &'a TrackId,
    payload: &'a [u8],
}

fn push_header(header: Header<'_>, out: &mut Vec<u8>) -> Result<()> {
    let Header {
        kind,
        flags,
        sequence,
        timestamp,
        decode,
        duration_ticks,
        track_id,
        payload,
    } = header;
    let track = track_id.as_str().as_bytes();
    if track.is_empty() || track.len() > MAX_TRACK_ID_BYTES {
        return Err(malformed());
    }
    if payload.is_empty() {
        return Err(malformed());
    }
    let max_payload = if kind == VIDEO {
        MAX_VIDEO_PAYLOAD_BYTES
    } else {
        MAX_AUDIO_PAYLOAD_BYTES
    };
    if payload.len() > max_payload || payload.len() > u32::MAX as usize {
        return Err(malformed());
    }
    out.extend(MAGIC);
    out.push(VERSION);
    out.push(kind);
    out.push(flags);
    out.push(0); // reserved for future per-frame flags
    out.extend(sequence.to_be_bytes());
    out.extend(timestamp.value().to_be_bytes());
    out.extend(decode.to_be_bytes());
    out.extend(timestamp.time_base().numerator().to_be_bytes());
    out.extend(timestamp.time_base().denominator().to_be_bytes());
    out.extend(duration_ticks.to_be_bytes());
    out.extend((track.len() as u16).to_be_bytes());
    out.extend((payload.len() as u32).to_be_bytes());
    out.extend(track);
    out.extend(payload);
    Ok(())
}

/// Encodes exactly one bounded media access unit.
pub fn encode(packet: &MediaPacket) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(HEADER_BYTES + packet.payload_len() + 32);
    match packet {
        MediaPacket::Video(frame) => {
            let mut flags = 0;
            let decode = if let Some(timestamp) = frame.decode() {
                flags |= HAS_DECODE_TIMESTAMP;
                if timestamp.time_base() != frame.presentation().time_base() {
                    return Err(malformed());
                }
                timestamp.value()
            } else {
                NO_DECODE_TIMESTAMP
            };
            if frame.is_keyframe() {
                flags |= KEYFRAME;
            }
            push_header(
                Header {
                    kind: VIDEO,
                    flags,
                    sequence: frame.sequence(),
                    timestamp: frame.presentation(),
                    decode,
                    duration_ticks: 0,
                    track_id: frame.track_id(),
                    payload: frame.payload(),
                },
                &mut out,
            )?;
        }
        MediaPacket::Audio(packet) => {
            push_header(
                Header {
                    kind: AUDIO,
                    flags: 0,
                    sequence: packet.sequence(),
                    timestamp: packet.timestamp(),
                    decode: NO_DECODE_TIMESTAMP,
                    duration_ticks: packet.duration_ticks(),
                    track_id: packet.track_id(),
                    payload: packet.payload(),
                },
                &mut out,
            )?;
        }
    }
    Ok(out)
}

/// Decodes exactly one bounded media access unit.
pub fn decode(bytes: &[u8]) -> Result<MediaPacket> {
    if bytes.len() < HEADER_BYTES {
        return Err(malformed());
    }
    let mut reader = Reader(bytes);
    if reader.take::<4>()? != MAGIC || reader.byte()? != VERSION {
        return Err(malformed());
    }
    let kind = reader.byte()?;
    let flags = reader.byte()?;
    if reader.byte()? != 0 {
        return Err(malformed());
    }
    let sequence = reader.u64()?;
    let presentation_value = reader.i64()?;
    let decode_value = reader.i64()?;
    let time_base = TimeBase::new(reader.u32()?, reader.u32()?).map_err(|_| malformed())?;
    let duration_ticks = reader.u32()?;
    let track_length = reader.u16()? as usize;
    let payload_length = reader.u32()? as usize;
    if track_length == 0 || track_length > MAX_TRACK_ID_BYTES {
        return Err(malformed());
    }
    let track =
        TrackId::new(String::from_utf8(reader.bytes(track_length)?).map_err(|_| malformed())?)
            .map_err(|_| malformed())?;
    let payload = reader.bytes(payload_length)?;
    reader.finish()?;
    let presentation = MediaTimestamp::new(presentation_value, time_base);
    match kind {
        VIDEO => {
            if flags & !(KEYFRAME | HAS_DECODE_TIMESTAMP) != 0 || duration_ticks != 0 {
                return Err(malformed());
            }
            if payload_length == 0 || payload_length > MAX_VIDEO_PAYLOAD_BYTES {
                return Err(malformed());
            }
            let decode = if flags & HAS_DECODE_TIMESTAMP != 0 {
                if decode_value == NO_DECODE_TIMESTAMP {
                    return Err(malformed());
                }
                Some(MediaTimestamp::new(decode_value, time_base))
            } else if decode_value == NO_DECODE_TIMESTAMP {
                None
            } else {
                return Err(malformed());
            };
            Ok(MediaPacket::Video(VideoFrame::new(
                track,
                sequence,
                presentation,
                decode,
                flags & KEYFRAME != 0,
                payload,
            )?))
        }
        AUDIO => {
            if flags != 0 || decode_value != NO_DECODE_TIMESTAMP || duration_ticks == 0 {
                return Err(malformed());
            }
            if payload_length == 0 || payload_length > MAX_AUDIO_PAYLOAD_BYTES {
                return Err(malformed());
            }
            Ok(MediaPacket::Audio(AudioPacket::new(
                track,
                sequence,
                presentation,
                duration_ticks,
                payload,
            )?))
        }
        _ => Err(malformed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time_base() -> TimeBase {
        TimeBase::new(1, 90_000).expect("valid time base")
    }

    #[test]
    fn video_round_trip_preserves_timestamps_and_keyframe() {
        let presentation = MediaTimestamp::new(900, time_base());
        let decode_ts = MediaTimestamp::new(870, time_base());
        let packet = MediaPacket::Video(
            VideoFrame::new(
                TrackId::new("video-0").unwrap(),
                7,
                presentation,
                Some(decode_ts),
                true,
                vec![1, 2, 3],
            )
            .unwrap(),
        );
        let decoded = decode(&encode(&packet).unwrap()).unwrap();
        assert_eq!(decoded, packet);
    }

    #[test]
    fn audio_round_trip_and_malformed_inputs_are_bounded() {
        let packet = MediaPacket::Audio(
            AudioPacket::new(
                TrackId::new("audio-0").unwrap(),
                8,
                MediaTimestamp::new(960, time_base()),
                960,
                vec![9, 8, 7],
            )
            .unwrap(),
        );
        let mut bytes = encode(&packet).unwrap();
        assert_eq!(decode(&bytes).unwrap(), packet);
        bytes[0] = b'X';
        assert!(decode(&bytes).is_err());
        let mut trailing = encode(&packet).unwrap();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }
}
