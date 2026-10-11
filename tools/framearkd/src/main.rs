//! Small local-network FANP reference receiver.
//!
//! `framearkd` exercises the real pairing, control, and FAM1 media paths with
//! bounded counting renderers. It is a protocol-lab daemon, not a hardware
//! decoder or a production authorization service.

use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use frameark_api::{AudioFrame, AudioRenderer, VideoFrame, VideoRenderer};
use frameark_core::{AudioConfig, DeviceId, Session, SessionId, VideoConfig};
use frameark_native::{
    MediaRendererFactory, NativeMediaControlReceiver, NativeMediaSessionReport, ReceiverPolicy,
    SessionOffer,
};
use frameark_transport::{CapabilityOffer, PairingCode, PairingServer};

const DEFAULT_BIND: &str = "0.0.0.0:4433";
const DEFAULT_SERVER_NAME: &str = "frameark.local";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
const MAX_CERTIFICATE_BYTES: usize = 16 * 1024;

#[derive(Debug)]
struct DaemonError(String);

impl Display for DaemonError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DaemonError {}

#[derive(Debug)]
struct Options {
    bind: SocketAddr,
    server_name: String,
    pairing_code: PairingCode,
    timeout: Duration,
    once: bool,
    certificate_out: Option<String>,
}

#[derive(Default)]
struct Counters {
    prepares: u64,
    starts: u64,
    resets: u64,
    video_frames: u64,
    audio_frames: u64,
    payload_bytes: u64,
}

#[derive(Clone)]
struct CountingVideo(Arc<Mutex<Counters>>);

impl VideoRenderer for CountingVideo {
    type Error = ();

    fn configure(&mut self, _config: VideoConfig) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn render(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
        let mut counters = self.0.lock().map_err(|_| ())?;
        counters.video_frames = counters.video_frames.saturating_add(1);
        counters.payload_bytes = counters
            .payload_bytes
            .saturating_add(frame.data.len() as u64);
        Ok(())
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        let mut counters = self.0.lock().map_err(|_| ())?;
        counters.resets = counters.resets.saturating_add(1);
        Ok(())
    }
}

#[derive(Clone)]
struct CountingAudio(Arc<Mutex<Counters>>);

impl AudioRenderer for CountingAudio {
    type Error = ();

    fn configure(&mut self, _config: AudioConfig) -> std::result::Result<(), Self::Error> {
        Ok(())
    }

    fn render(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
        let mut counters = self.0.lock().map_err(|_| ())?;
        counters.audio_frames = counters.audio_frames.saturating_add(1);
        counters.payload_bytes = counters
            .payload_bytes
            .saturating_add(frame.data.len() as u64);
        Ok(())
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        let mut counters = self.0.lock().map_err(|_| ())?;
        counters.resets = counters.resets.saturating_add(1);
        Ok(())
    }
}

struct CountingFactory(Arc<Mutex<Counters>>);

impl MediaRendererFactory for CountingFactory {
    type Video = CountingVideo;
    type Audio = CountingAudio;

    fn prepare(
        &mut self,
        offer: SessionOffer,
    ) -> frameark_core::Result<frameark_native::media_session::MediaSession<Self::Video, Self::Audio>>
    {
        let counters = Arc::clone(&self.0);
        let mut counters_guard = counters.lock().map_err(|_| {
            frameark_core::FrameArkError::invalid_state(
                "framearkd.counters",
                "counter lock is unavailable",
            )
        })?;
        counters_guard.prepares = counters_guard.prepares.saturating_add(1);
        drop(counters_guard);
        frameark_native::media_session::MediaSession::prepare(
            offer,
            offer.video.map(|_| {
                (
                    frameark_core::TrackId::new("video-0").unwrap(),
                    CountingVideo(counters.clone()),
                )
            }),
            offer.audio.map(|_| {
                (
                    frameark_core::TrackId::new("audio-0").unwrap(),
                    CountingAudio(counters),
                )
            }),
        )
    }
}

struct CountingBackend(Arc<Mutex<Counters>>);

impl frameark_native::SessionBackend for CountingBackend {
    fn prepare(&mut self, _offer: SessionOffer) -> frameark_core::Result<()> {
        Ok(())
    }

    fn start(&mut self) -> frameark_core::Result<()> {
        let mut counters = self.0.lock().map_err(|_| {
            frameark_core::FrameArkError::invalid_state(
                "framearkd.counters",
                "counter lock is unavailable",
            )
        })?;
        counters.starts = counters.starts.saturating_add(1);
        Ok(())
    }

    fn reset(&mut self) -> frameark_core::Result<()> {
        Ok(())
    }
}

fn usage() -> &'static str {
    "usage: framearkd [--bind HOST:PORT] [--server-name NAME] \
     [--pairing-code CODE] [--timeout-ms N] [--certificate-out PATH] [--once]"
}

fn option_value(args: &[String], name: &str) -> std::result::Result<Option<String>, DaemonError> {
    let Some(index) = args.iter().position(|value| value == name) else {
        return Ok(None);
    };
    args.get(index + 1)
        .cloned()
        .map(Some)
        .ok_or_else(|| DaemonError(format!("missing value for {name}")))
}

fn parse_options(args: &[String]) -> std::result::Result<Options, DaemonError> {
    if args.iter().any(|value| value == "--help" || value == "-h") {
        return Err(DaemonError(usage().to_string()));
    }
    let bind = option_value(args, "--bind")?
        .unwrap_or_else(|| DEFAULT_BIND.to_string())
        .parse::<SocketAddr>()
        .map_err(|_| DaemonError("--bind must be a valid socket address".to_string()))?;
    let server_name =
        option_value(args, "--server-name")?.unwrap_or_else(|| DEFAULT_SERVER_NAME.to_string());
    let pairing_code = match option_value(args, "--pairing-code")? {
        Some(value) => PairingCode::parse(value)
            .map_err(|_| DaemonError("--pairing-code must be six decimal digits".to_string()))?,
        None => PairingCode::generate(),
    };
    let timeout_ms = option_value(args, "--timeout-ms")?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| DaemonError("--timeout-ms must be an integer".to_string()))
        })
        .transpose()?
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    if !(100..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(DaemonError(format!(
            "--timeout-ms must be in 100..={MAX_TIMEOUT_MS}"
        )));
    }
    Ok(Options {
        bind,
        server_name,
        pairing_code,
        timeout: Duration::from_millis(timeout_ms),
        once: args.iter().any(|value| value == "--once"),
        certificate_out: option_value(args, "--certificate-out")?,
    })
}

fn write_certificate(path: &str, certificate: &[u8]) -> std::result::Result<(), DaemonError> {
    if certificate.is_empty() || certificate.len() > MAX_CERTIFICATE_BYTES {
        return Err(DaemonError(
            "generated certificate exceeds its bound".to_string(),
        ));
    }
    fs::write(path, certificate)
        .map_err(|_| DaemonError("certificate output could not be written".to_string()))
}

fn print_report(report: &NativeMediaSessionReport, counters: &Arc<Mutex<Counters>>) {
    let counters = counters.lock().ok();
    println!(
        "session-complete state={:?} requests={} media_frames={} video_frames={} audio_frames={} payload_bytes={} resets={}",
        report.control.state,
        report.control.requests,
        report.media.frames,
        report.media.video_frames,
        report.media.audio_frames,
        report.media.payload_bytes,
        counters.as_ref().map_or(0, |value| value.resets),
    );
}

async fn run(options: Options) -> std::result::Result<(), Box<dyn Error>> {
    let server = Arc::new(
        PairingServer::bind_with_capabilities(
            options.bind,
            options.server_name.clone(),
            options.pairing_code.clone(),
            CapabilityOffer::default_capabilities(),
        )
        .map_err(|_| DaemonError("FANP listener could not start".to_string()))?,
    );
    let address = server
        .local_addr()
        .map_err(|_| DaemonError("FANP listener address is unavailable".to_string()))?;
    if let Some(path) = &options.certificate_out {
        write_certificate(path, server.certificate_der())?;
    }
    println!(
        "listening address={} server_name={} pairing_code={} certificate_bytes={} once={}",
        address,
        options.server_name,
        options.pairing_code.as_str(),
        server.certificate_der().len(),
        options.once,
    );
    loop {
        let transport = match server.accept_pairing(options.timeout).await {
            Ok(transport) => transport,
            Err(_) if !options.once => {
                eprintln!("pairing-attempt-rejected");
                continue;
            }
            Err(_) => return Err(DaemonError("FANP pairing failed".to_string()).into()),
        };
        let counters = Arc::new(Mutex::new(Counters::default()));
        let receiver = NativeMediaControlReceiver::new(
            transport,
            Session::new(
                SessionId::try_from("framearkd-session")
                    .map_err(|_| DaemonError("invalid daemon session".to_string()))?,
                DeviceId::try_from("framearkd")
                    .map_err(|_| DaemonError("invalid daemon device".to_string()))?,
            ),
            ReceiverPolicy {
                video: true,
                opus: true,
                aac: true,
                max_width: 3840,
                max_height: 2160,
                max_fps: 60,
                max_channels: 8,
            },
            CountingBackend(Arc::clone(&counters)),
            CountingFactory(Arc::clone(&counters)),
        )
        .map_err(|_| DaemonError("FANP receiver policy was rejected".to_string()))?;
        match receiver.serve(options.timeout).await {
            Ok(report) => print_report(&report, &counters),
            Err(_) => eprintln!("session-failed; resources-released=true"),
        }
        if options.once {
            break;
        }
    }
    server.close();
    Ok(())
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|value| value == "--help" || value == "-h") {
        println!("{}", usage());
        return Ok(());
    }
    run(parse_options(&args)?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_generate_a_bounded_local_receiver_configuration() {
        let options = parse_options(&[]).unwrap();
        assert_eq!(options.bind, DEFAULT_BIND.parse().unwrap());
        assert_eq!(options.server_name, DEFAULT_SERVER_NAME);
        assert_eq!(options.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        assert!(!options.once);
    }

    #[test]
    fn options_reject_invalid_pairing_and_timeout_values() {
        assert!(parse_options(&["--pairing-code".to_string(), "12".to_string(),]).is_err());
        assert!(parse_options(&["--timeout-ms".to_string(), "99".to_string(),]).is_err());
    }

    #[test]
    fn usage_exposes_reference_receiver_controls() {
        let text = usage();
        for option in ["--bind", "--server-name", "--pairing-code", "--once"] {
            assert!(text.contains(option));
        }
    }
}
