//! Small, deliberately explicit FANP fixture sender.
//!
//! The CLI accepts real encoded access-unit fixture files; it does not contain
//! a codec or silently label arbitrary bytes as decodable video. It provides
//! bounded discovery and FANP lifecycle commands for lab and receiver
//! diagnostics while platform playback remains outside the CLI.

use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use frameark_core::{
    AudioConfig, DeviceId, MediaCodec, Session, SessionId, TimeBase, TrackId, VideoConfig,
};
use frameark_discovery::{DiscoveryConfig, DiscoveryEvent, MdnsDiscovery};
use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame};
use frameark_native::{NativeSender, SessionOffer, media_wire};
use frameark_transport::{PairingClient, PairingCode};

const MAX_FRAMES: usize = 10_000;
const MAX_FIXTURE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_INTERVAL_MS: u64 = 33;
const MAX_DISCOVERY_EVENTS: usize = 256;
const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 3_000;

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
    audio_codec: MediaCodec,
    frames: usize,
    interval: Duration,
}

#[derive(Debug)]
struct PairingOptions {
    address: SocketAddr,
    server_name: String,
    certificate: Vec<u8>,
    pairing_code: PairingCode,
}

#[derive(Debug)]
struct DiscoverOptions {
    timeout: Duration,
    max_events: usize,
}

fn usage() -> &'static str {
    "usage: frameark-cli <discover|connect|status|end|send> [options]\n\
     discover [--timeout-ms N] [--max-events N]\n\
     connect|status|end --host IP --port PORT --server-name NAME \
     --certificate PATH --pairing-code CODE\n\
     send --host IP --port PORT --server-name NAME --certificate PATH \
     --pairing-code CODE [--video-fixture PATH] [--audio-fixture PATH] \
     [--frames N] [--interval-ms N]"
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

fn parse_u64(value: &str, name: &str, maximum: u64) -> Result<u64, CliError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| CliError(format!("{name} must be an integer")))?;
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

fn parse_pairing_options(args: &[String]) -> Result<PairingOptions, CliError> {
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
    let certificate = read_fixture(&required_value(args, "--certificate")?, 16 * 1024)?;
    let pairing_code = PairingCode::parse(required_value(args, "--pairing-code")?)
        .map_err(|_| CliError("--pairing-code must be a six-digit code".to_string()))?;
    Ok(PairingOptions {
        address: SocketAddr::new(host, port),
        server_name,
        certificate,
        pairing_code,
    })
}

fn parse_discover_options(args: &[String]) -> Result<DiscoverOptions, CliError> {
    let timeout_ms = option_value(args, "--timeout-ms")?
        .map(|value| parse_u64(&value, "--timeout-ms", 60_000))
        .transpose()?
        .unwrap_or(DEFAULT_DISCOVERY_TIMEOUT_MS);
    let max_events = option_value(args, "--max-events")?
        .map(|value| parse_usize(&value, "--max-events", MAX_DISCOVERY_EVENTS))
        .transpose()?
        .unwrap_or(MAX_DISCOVERY_EVENTS);
    Ok(DiscoverOptions {
        timeout: Duration::from_millis(timeout_ms),
        max_events,
    })
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
    let audio_codec = match option_value(args, "--audio-codec")?.as_deref() {
        None | Some("opus") => MediaCodec::Opus,
        Some("aac") => MediaCodec::Aac,
        Some(_) => return Err(CliError("--audio-codec must be opus or aac".to_string())),
    };
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
        audio_codec,
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
    let mut sender = NativeSender::new(
        transport,
        Session::new(
            SessionId::try_from("cli-session")
                .map_err(|_| CliError("invalid CLI session".into()))?,
            DeviceId::try_from("frameark-cli")
                .map_err(|_| CliError("invalid CLI device".into()))?,
        ),
    )
    .map_err(|_| CliError("FANP session could not be created".to_string()))?;
    sender
        .offer(
            SessionOffer {
                video: options.video.as_ref().map(|_| VideoConfig {
                    codec: MediaCodec::H264,
                    width: 1280,
                    height: 720,
                    frame_rate_numerator: 30,
                    frame_rate_denominator: 1,
                }),
                audio: options.audio.as_ref().map(|_| AudioConfig {
                    codec: options.audio_codec,
                    sample_rate: 48_000,
                    channels: 2,
                }),
                latency_ms: 120,
            },
            Duration::from_secs(5),
        )
        .await
        .map_err(|_| CliError("FANP media offer was rejected".to_string()))?;
    sender
        .start(Duration::from_secs(5))
        .await
        .map_err(|_| CliError("FANP media start was rejected".to_string()))?;
    let mut stream = sender
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
    sender
        .stop(Duration::from_secs(5))
        .await
        .map_err(|_| CliError("FANP media stop was rejected".to_string()))?;
    Ok(())
}

async fn connect_sender(options: PairingOptions) -> Result<NativeSender, Box<dyn Error>> {
    let transport = PairingClient::connect(
        options.address,
        &options.server_name,
        &options.certificate,
        options.pairing_code,
        Duration::from_secs(5),
    )
    .await
    .map_err(|_| CliError("FANP pairing or capability negotiation failed".to_string()))?;
    NativeSender::new(
        transport,
        Session::new(
            SessionId::try_from("cli-management")
                .map_err(|_| CliError("invalid CLI session".to_string()))?,
            DeviceId::try_from("frameark-cli")
                .map_err(|_| CliError("invalid CLI device".to_string()))?,
        ),
    )
    .map_err(|_| CliError("FANP session could not be created".to_string()).into())
}

async fn connect(options: PairingOptions) -> Result<(), Box<dyn Error>> {
    let mut sender = connect_sender(options).await?;
    println!("connected state={:?}", sender.state());
    sender.abort();
    Ok(())
}

async fn status(options: PairingOptions) -> Result<(), Box<dyn Error>> {
    let mut sender = connect_sender(options).await?;
    let state = sender
        .query_status(Duration::from_secs(5))
        .await
        .map_err(|_| CliError("FANP status request failed".to_string()))?;
    println!("status state={state:?}");
    sender.abort();
    Ok(())
}

async fn end(options: PairingOptions) -> Result<(), Box<dyn Error>> {
    let mut sender = connect_sender(options).await?;
    let report = sender
        .stop(Duration::from_secs(5))
        .await
        .map_err(|_| CliError("FANP end request failed".to_string()))?;
    println!(
        "ended state={:?} requests={}",
        report.state, report.requests
    );
    Ok(())
}

fn print_discovery_event(event: DiscoveryEvent) {
    match event {
        DiscoveryEvent::SearchStarted => println!("search-started"),
        DiscoveryEvent::SearchStopped => println!("search-stopped"),
        DiscoveryEvent::ServiceFound { fullname } => println!("service-found {fullname}"),
        DiscoveryEvent::ServiceRemoved { fullname } => println!("service-removed {fullname}"),
        DiscoveryEvent::ServiceResolved(service) => println!(
            "service-resolved fullname={} host={} port={} addresses={:?} properties={:?}",
            service.fullname, service.host, service.port, service.addresses, service.properties
        ),
        DiscoveryEvent::BackendIgnored => println!("backend-event-ignored"),
    }
}

fn discover(options: DiscoverOptions) -> Result<(), Box<dyn Error>> {
    let discovery = MdnsDiscovery::new(DiscoveryConfig::all_interfaces())
        .map_err(|_| CliError("mDNS discovery could not start".to_string()))?;
    let browser = discovery
        .browse()
        .map_err(|_| CliError("mDNS browse could not start".to_string()))?;
    let deadline = std::time::Instant::now() + options.timeout;
    let mut events = 0;
    while events < options.max_events {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        if let Some(event) = browser.recv_timeout(remaining.min(Duration::from_millis(250)))? {
            print_discovery_event(event);
            events += 1;
        }
    }
    discovery
        .shutdown()
        .map_err(|_| CliError("mDNS discovery shutdown failed".to_string()))?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", usage());
        return Ok(());
    }
    match args[0].as_str() {
        "discover" => discover(parse_discover_options(&args[1..])?),
        "connect" => connect(parse_pairing_options(&args[1..])?).await,
        "status" => status(parse_pairing_options(&args[1..])?).await,
        "end" => end(parse_pairing_options(&args[1..])?).await,
        "send" => send(parse_options(&args[1..])?).await,
        _ => Err(CliError(format!("unknown command {}; {}", args[0], usage())).into()),
    }
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

    #[test]
    fn management_options_are_bounded_and_pairing_requires_all_fields() {
        let discover = parse_discover_options(&[]).unwrap();
        assert_eq!(
            discover.timeout,
            Duration::from_millis(DEFAULT_DISCOVERY_TIMEOUT_MS)
        );
        assert_eq!(discover.max_events, MAX_DISCOVERY_EVENTS);
        assert!(
            parse_discover_options(&["--timeout-ms".to_string(), "60001".to_string(),]).is_err()
        );
        assert!(parse_pairing_options(&[]).is_err());
    }

    #[test]
    fn management_usage_names_all_lifecycle_commands() {
        let text = usage();
        for command in ["discover", "connect", "status", "end", "send"] {
            assert!(text.contains(command));
        }
    }
}
