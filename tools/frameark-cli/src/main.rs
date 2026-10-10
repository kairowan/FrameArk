//! Small, deliberately explicit FANP fixture sender.
//!
//! The CLI accepts real encoded access-unit fixture files; it does not contain
//! a codec or silently label arbitrary bytes as decodable video. Its current
//! media-only command is intended for the Rust loopback receiver and lab
//! workflows while the full control-plane CLI is still being implemented.

use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use frameark_core::{TimeBase, TrackId};
use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame};
use frameark_native::media_wire;
use frameark_transport::{PairingClient, PairingCode};

const MAX_FRAMES: usize = 10_000;
const MAX_FIXTURE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_INTERVAL_MS: u64 = 33;

#[derive(Debug)]
struct CliError(String);

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

#[derive(Debug)]
struct SendOptions {
    address: SocketAddr,
    server_name: String,
    certificate: Vec<u8>,
    pairing_code: PairingCode,
    video: Option<Vec<u8>>,
    audio: Option<Vec<u8>>,
    frames: usize,
    interval: Duration,
}

fn usage() -> &'static str {
    "usage: frameark-cli send --host IP --port PORT --server-name NAME \
     --certificate PATH --pairing-code CODE [--video-fixture PATH] \
     [--audio-fixture PATH] [--frames N] [--interval-ms N]"
}

fn option_value(args: &[String], name: &str) -> Result<Option<String>, CliError> {
    let Some(index) = args.iter().position(|value| value == name) else {
        return Ok(None);
    };
    args.get(index + 1)
        .cloned()
        .map(Some)
        .ok_or_else(|| CliError(format!("missing value for {name}")))
}

fn required_value(args: &[String], name: &str) -> Result<String, CliError> {
    option_value(args, name)?.ok_or_else(|| CliError(format!("missing required option {name}")))
}

fn parse_usize(value: &str, name: &str, maximum: usize) -> Result<usize, CliError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| CliError(format!("{name} must be a positive integer")))?;
    if parsed == 0 || parsed > maximum {
        return Err(CliError(format!("{name} must be in 1..={maximum}")));
    }
    Ok(parsed)
}

fn read_fixture(path: &str, maximum: usize) -> Result<Vec<u8>, CliError> {
    let path_ref = Path::new(path);
    let metadata = fs::metadata(path_ref)
        .map_err(|_| CliError("fixture file cannot be inspected".to_string()))?;
    let length = usize::try_from(metadata.len())
        .map_err(|_| CliError("fixture file is too large".to_string()))?;
    if length == 0 || length > maximum {
        return Err(CliError(
            "fixture file is empty or exceeds the media limit".to_string(),
        ));
    }
    fs::read(path_ref).map_err(|_| CliError("fixture file cannot be read".to_string()))
}

fn parse_options(args: &[String]) -> Result<SendOptions, CliError> {
    let host = required_value(args, "--host")?
        .parse::<IpAddr>()
        .map_err(|_| CliError("--host must be an IP address".to_string()))?;
    let port = required_value(args, "--port")?
        .parse::<u16>()
        .map_err(|_| CliError("--port must be a non-zero port".to_string()))?;
    if port == 0 {
        return Err(CliError("--port must be a non-zero port".to_string()));
    }
    let server_name = required_value(args, "--server-name")?;
    let certificate_path = required_value(args, "--certificate")?;
    let certificate = read_fixture(&certificate_path, 16 * 1024)?;
    let pairing_code = PairingCode::parse(required_value(args, "--pairing-code")?)
        .map_err(|_| CliError("--pairing-code must be a six-digit code".to_string()))?;
    let video = option_value(args, "--video-fixture")?
        .map(|path| read_fixture(&path, MAX_FIXTURE_BYTES))
        .transpose()?;
    let audio = option_value(args, "--audio-fixture")?
        .map(|path| read_fixture(&path, 256 * 1024))
        .transpose()?;
    if video.is_none() && audio.is_none() {
        return Err(CliError(
            "at least one encoded fixture is required (--video-fixture or --audio-fixture)"
                .to_string(),
        ));
    }
    let frames = option_value(args, "--frames")?
        .map(|value| parse_usize(&value, "--frames", MAX_FRAMES))
        .transpose()?
        .unwrap_or(1);
    let interval_ms = option_value(args, "--interval-ms")?
        .map(|value| parse_usize(&value, "--interval-ms", 10_000))
        .transpose()?
        .unwrap_or(DEFAULT_INTERVAL_MS as usize);
    Ok(SendOptions {
        address: SocketAddr::new(host, port),
        server_name,
        certificate,
        pairing_code,
        video,
        audio,
        frames,
        interval: Duration::from_millis(interval_ms as u64),
    })
}

async fn send(options: SendOptions) -> Result<(), Box<dyn Error>> {
    let transport = PairingClient::connect(
        options.address,
        &options.server_name,
        &options.certificate,
        options.pairing_code,
        Duration::from_secs(5),
    )
    .await
    .map_err(|_| CliError("FANP pairing or capability negotiation failed".to_string()))?;
    let mut stream = transport
        .open_media_stream()
        .await
        .map_err(|_| CliError("FANP media stream could not be opened".to_string()))?;
    let time_base = TimeBase::new(1, 90_000).map_err(|_| CliError("invalid time base".into()))?;
    for sequence in 0..options.frames as u64 {
        let timestamp = MediaTimestamp::new(sequence.saturating_mul(3_000) as i64, time_base);
        if let Some(payload) = &options.video {
            let packet = MediaPacket::Video(VideoFrame::new(
                TrackId::new("video-0").map_err(|_| CliError("invalid video track".into()))?,
                sequence,
                timestamp,
                None,
                sequence == 0,
                payload.clone(),
            )?);
            let encoded = media_wire::encode(&packet)?;
            stream.send_frame(&encoded, Duration::from_secs(2)).await?;
        }
        if let Some(payload) = &options.audio {
            let packet = MediaPacket::Audio(AudioPacket::new(
                TrackId::new("audio-0").map_err(|_| CliError("invalid audio track".into()))?,
                sequence,
                timestamp,
                960,
                payload.clone(),
            )?);
            let encoded = media_wire::encode(&packet)?;
            stream.send_frame(&encoded, Duration::from_secs(2)).await?;
        }
        if sequence + 1 < options.frames as u64 {
            tokio::time::sleep(options.interval).await;
        }
    }
    stream.finish()?;
    transport.close();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", usage());
        return Ok(());
    }
    if args[0] != "send" {
        return Err(CliError(format!("unknown command {}; {}", args[0], usage())).into());
    }
    let options = parse_options(&args[1..])?;
    send(options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_and_interval_are_bounded() {
        assert_eq!(parse_usize("3", "--frames", MAX_FRAMES).unwrap(), 3);
        assert!(parse_usize("0", "--frames", MAX_FRAMES).is_err());
        assert!(parse_usize("10001", "--interval-ms", 10_000).is_err());
    }

    #[test]
    fn send_requires_a_real_fixture_path() {
        let args = vec![
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            "4433".to_string(),
            "--server-name".to_string(),
            "localhost".to_string(),
            "--certificate".to_string(),
            "missing.der".to_string(),
            "--pairing-code".to_string(),
            "123456".to_string(),
        ];
        assert!(parse_options(&args).is_err());
    }
}
