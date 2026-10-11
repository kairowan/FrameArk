//! JNI entry points kept deliberately small while the Android media bridge is
//! introduced in bounded lifecycle increments.
//!
//! This crate is the only workspace exception to the `unsafe_code` lint because
//! Rust 2024 requires an unsafe attribute for the exported JNI symbol. The
//! function body itself does not dereference raw pointers or access JNI state.

#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use frameark_api::{AudioFrame, AudioRenderer, EncodedMediaSink, VideoFrame, VideoRenderer};
use frameark_core::{
    AudioConfig, CORE_ABI_VERSION, DeviceId, MediaCodec, Session, SessionId, SessionState,
    VideoConfig,
};
use frameark_discovery::{DiscoveryConfig, MdnsDiscovery, PublishedRegistration, PublishedService};
use frameark_native::media_session::MediaSession;
use frameark_native::{
    MediaRendererFactory, NativeMediaControlReceiver, ReceiverPolicy, SessionBackend, SessionOffer,
};
use frameark_transport::{PairingCode, PairingServer};
use jni::{
    JNIEnv,
    objects::{JByteArray, JObject},
    sys::{jboolean, jbyteArray, jint, jlong},
};

/// Maximum encoded frame accepted at the JNI boundary.
pub const MAX_JNI_MEDIA_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// Maximum encoded frames retained before the producer receives backpressure.
pub const MAX_JNI_MEDIA_QUEUE_FRAMES: usize = 16;
const MEDIA_FRAME_MAGIC: &[u8; 4] = b"FAMF";
const MEDIA_FRAME_VERSION: u8 = 1;
const MEDIA_KIND_VIDEO: u8 = 1;
const MEDIA_KIND_AUDIO: u8 = 2;
const MEDIA_CONFIG_MAGIC: &[u8; 4] = b"FAMC";
const MEDIA_CONFIG_VERSION: u8 = 1;
const NETWORK_SERVER_NAME: &str = "frameark.local";
const DISCOVERY_INSTANCE_NAME: &str = "FrameArk Receiver";
const DISCOVERY_HOST_NAME: &str = "frameark.local.";
const NETWORK_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the core ABI version expected by the Android receiver.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeVersion(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    CORE_ABI_VERSION as jint
}

#[derive(Default)]
struct NativeReceiverState {
    session: Option<Session>,
    media_queue: VecDeque<NativeMediaFrame>,
    video_config: Option<VideoConfig>,
    audio_config: Option<AudioConfig>,
    network: Option<NetworkRuntime>,
}

struct NetworkRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    discovery: Option<DiscoveryRuntime>,
    port: u16,
    pairing_code: u32,
    certificate_der: Vec<u8>,
}

struct DiscoveryRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl DiscoveryRuntime {
    fn start(port: u16) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("frameark-fanp-discovery".to_string())
            .spawn(move || {
                let Ok(discovery) = MdnsDiscovery::new(DiscoveryConfig::all_interfaces()) else {
                    let _ = ready_sender.send(false);
                    return;
                };
                let Ok(service) =
                    PublishedService::new(DISCOVERY_INSTANCE_NAME, DISCOVERY_HOST_NAME, port)
                        .and_then(|service| service.with_property("proto", "fanp1"))
                        .and_then(|service| service.with_property("ver", "0.1"))
                        .and_then(|service| service.with_property("tls", "pin"))
                        .and_then(|service| service.with_property("media", "h264,opus,aac"))
                else {
                    let _ = ready_sender.send(false);
                    return;
                };
                let Ok(registration) = discovery.publish(&service) else {
                    let _ = ready_sender.send(false);
                    return;
                };
                if ready_sender.send(true).is_err() {
                    return;
                }
                while !stop_thread.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(250));
                }
                unregister_discovery(registration, discovery);
            })
            .ok()?;
        if ready_receiver.recv().ok() != Some(true) {
            let _ = join.join();
            return None;
        }
        Some(Self {
            stop,
            join: Some(join),
        })
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn unregister_discovery(registration: PublishedRegistration, discovery: MdnsDiscovery) {
    let _ = registration.unregister();
    let _ = discovery.shutdown();
}

impl NetworkRuntime {
    fn start() -> Result<Self, String> {
        let pairing_code = PairingCode::generate();
        let pairing_value = pairing_code
            .as_str()
            .parse()
            .map_err(|_| "FANP pairing code".to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("frameark-fanp-receiver".to_string())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    let _ = ready_sender.send(Err("FANP runtime".to_string()));
                    return;
                };
                let bind_result = {
                    let _entered = runtime.enter();
                    let bind_address = if cfg!(test) {
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
                    } else {
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
                    };
                    PairingServer::bind(
                        bind_address,
                        if cfg!(test) {
                            "localhost"
                        } else {
                            NETWORK_SERVER_NAME
                        },
                        pairing_code,
                    )
                };
                let server = match bind_result {
                    Ok(server) => server,
                    Err(error) => {
                        let _ = ready_sender.send(Err(format!("FANP listener: {error:?}")));
                        return;
                    }
                };
                let Ok(port) = server.local_addr().map(|address| address.port()) else {
                    let _ = ready_sender.send(Err("FANP address".to_string()));
                    return;
                };
                let certificate_der = server.certificate_der().to_vec();
                if ready_sender.send(Ok((port, certificate_der))).is_err() {
                    return;
                }
                runtime.block_on(async move {
                    while !stop_thread.load(Ordering::Acquire) {
                        let accepted = tokio::time::timeout(
                            Duration::from_millis(250),
                            server.accept_pairing(NETWORK_OPERATION_TIMEOUT),
                        )
                        .await;
                        let transport = match accepted {
                            Ok(Ok(transport)) => transport,
                            Ok(Err(_)) | Err(_) => continue,
                        };
                        let receiver = match NativeMediaControlReceiver::new(
                            transport,
                            network_session(),
                            network_policy(),
                            NetworkBackend,
                            NetworkRendererFactory,
                        ) {
                            Ok(receiver) => receiver,
                            Err(_) => continue,
                        };
                        let _ = receiver.serve(NETWORK_OPERATION_TIMEOUT).await;
                    }
                    server.close();
                });
            })
            .map_err(|_| "FANP worker".to_string())?;
        let (port, certificate_der) = ready_receiver
            .recv()
            .map_err(|_| "FANP startup".to_string())??;
        let discovery = DiscoveryRuntime::start(port);
        Ok(Self {
            stop,
            join: Some(join),
            discovery,
            port,
            pairing_code: pairing_value,
            certificate_der,
        })
    }

    fn stop(mut self) {
        if let Some(discovery) = self.discovery.take() {
            discovery.stop();
        }
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn network_session() -> Session {
    Session::new(
        SessionId::try_from("android-network-session").expect("static session id"),
        DeviceId::try_from("android-device").expect("static device id"),
    )
}

const fn network_policy() -> ReceiverPolicy {
    ReceiverPolicy {
        video: true,
        opus: true,
        aac: true,
        max_width: 3840,
        max_height: 2160,
        max_fps: 60,
        max_channels: 8,
    }
}

struct NetworkBackend;

impl SessionBackend for NetworkBackend {
    fn prepare(&mut self, _offer: SessionOffer) -> frameark_core::Result<()> {
        Ok(())
    }

    fn start(&mut self) -> frameark_core::Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> frameark_core::Result<()> {
        Ok(())
    }
}

struct NetworkVideoRenderer;

impl VideoRenderer for NetworkVideoRenderer {
    type Error = ();

    fn configure(&mut self, config: VideoConfig) -> std::result::Result<(), Self::Error> {
        set_video_config(config);
        Ok(())
    }

    fn render(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_VIDEO,
            pts: frame.timestamp,
            keyframe: frame.keyframe,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

struct NetworkAudioRenderer;

impl AudioRenderer for NetworkAudioRenderer {
    type Error = ();

    fn configure(&mut self, config: AudioConfig) -> std::result::Result<(), Self::Error> {
        set_audio_config(config);
        Ok(())
    }

    fn render(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_AUDIO,
            pts: frame.timestamp,
            keyframe: false,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }

    fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

struct NetworkRendererFactory;

impl MediaRendererFactory for NetworkRendererFactory {
    type Video = NetworkVideoRenderer;
    type Audio = NetworkAudioRenderer;

    fn prepare(
        &mut self,
        offer: SessionOffer,
    ) -> frameark_core::Result<MediaSession<Self::Video, Self::Audio>> {
        MediaSession::prepare(
            offer,
            offer.video.map(|_| {
                (
                    frameark_core::TrackId::new("video-0").expect("static track id"),
                    NetworkVideoRenderer,
                )
            }),
            offer.audio.map(|_| {
                (
                    frameark_core::TrackId::new("audio-0").expect("static track id"),
                    NetworkAudioRenderer,
                )
            }),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeMediaFrame {
    kind: u8,
    pts: i64,
    keyframe: bool,
    payload: Vec<u8>,
}

/// Encoded FANP sink that feeds the process-local JNI polling queue.
///
/// A native network/session coordinator can pass this sink to
/// `frameark_native::NativeEncodedMediaReceiver`; Android then polls the same
/// queue and submits the returned envelopes to `MediaCodec`/`AudioTrack`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JniMediaQueueSink;

impl EncodedMediaSink for JniMediaQueueSink {
    type Error = ();

    fn push_video(&mut self, frame: VideoFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_VIDEO,
            pts: frame.timestamp,
            keyframe: frame.keyframe,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }

    fn push_audio(&mut self, frame: AudioFrame<'_>) -> std::result::Result<(), Self::Error> {
        if submit_media_frame(NativeMediaFrame {
            kind: MEDIA_KIND_AUDIO,
            pts: frame.timestamp,
            keyframe: false,
            payload: frame.data.to_vec(),
        }) == 0
        {
            Ok(())
        } else {
            Err(())
        }
    }
}

static RECEIVER_STATE: OnceLock<Mutex<NativeReceiverState>> = OnceLock::new();

fn receiver_state() -> &'static Mutex<NativeReceiverState> {
    RECEIVER_STATE.get_or_init(|| Mutex::new(NativeReceiverState::default()))
}

/// Starts the Rust-owned receiver lifecycle and returns 0 on success.
///
/// A return value of 1 means the receiver is already started; negative values
/// indicate an internal lock or session construction failure. Network
/// discovery and media sockets are deliberately separate increments.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeStartReceiver(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    if state.session.is_some() {
        return 1;
    }
    state.media_queue.clear();
    state.video_config = None;
    state.audio_config = None;
    let Ok(session_id) = SessionId::try_from("android-receiver") else {
        return -1;
    };
    let Ok(device_id) = DeviceId::try_from("android-device") else {
        return -1;
    };
    let mut session = Session::new(session_id, device_id);
    if session.transition(SessionState::Connecting).is_err() {
        return -1;
    }
    let Ok(network) = NetworkRuntime::start() else {
        return -4;
    };
    state.session = Some(session);
    state.network = Some(network);
    0
}

/// Stops the Rust-owned receiver lifecycle and returns 0 on success.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeStopReceiver(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    let Some(mut session) = state.session.take() else {
        return 0;
    };
    state.media_queue.clear();
    state.video_config = None;
    state.audio_config = None;
    let network = state.network.take();
    let transition_failed = session.transition(SessionState::Closed).is_err();
    drop(state);
    if let Some(network) = network {
        network.stop();
    }
    if transition_failed {
        return -1;
    }
    0
}

/// Returns 1 while the Rust receiver lifecycle is active, otherwise 0.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverRunning(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    if state.session.is_some() { 1 } else { 0 }
}

/// Returns the ephemeral FANP listener port, or zero when the receiver is stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverPort(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    state
        .network
        .as_ref()
        .map_or(0, |network| i32::from(network.port))
}

/// Returns the temporary six-digit pairing code, or zero when stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverPairingCode(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    let Ok(state) = receiver_state().lock() else {
        return -1;
    };
    state
        .network
        .as_ref()
        .map_or(0, |network| network.pairing_code as jint)
}

/// Returns the pinned ephemeral server certificate, or null when stopped.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeReceiverCertificate(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Ok(state) = receiver_state().lock() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let Some(network) = state.network.as_ref() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&network.certificate_der) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Queues one encoded video access unit for the Android renderer boundary.
///
/// A return value of 0 means accepted, 1 means bounded backpressure, and
/// negative values mean that the receiver is stopped or the frame is invalid.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSubmitVideoFrame(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    payload: JByteArray<'_>,
    pts: jlong,
    keyframe: jboolean,
) -> jint {
    submit_java_frame(&mut env, payload, MEDIA_KIND_VIDEO, pts, keyframe != 0)
}

/// Queues one encoded audio access unit for the Android renderer boundary.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeSubmitAudioFrame(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    payload: JByteArray<'_>,
    pts: jlong,
) -> jint {
    submit_java_frame(&mut env, payload, MEDIA_KIND_AUDIO, pts, false)
}

/// Takes the oldest queued encoded frame as a bounded `FAMF` envelope.
///
/// The envelope is `FAMF`, version, kind, keyframe flag, signed-big-endian
/// PTS, u32 payload length, and payload bytes. A null Java byte array means the
/// queue is empty.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollMediaFrame(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Ok(mut state) = receiver_state().lock() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let Some(frame) = state.media_queue.pop_front() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    let encoded = encode_media_frame(&frame);
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Takes the newest negotiated video configuration as a bounded `FAMC` envelope.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollVideoConfig(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(encoded) = take_video_config() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

/// Takes the newest negotiated audio configuration as a bounded `FAMC` envelope.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePollAudioConfig(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jbyteArray {
    let Some(encoded) = take_audio_config() else {
        return JObject::null().into_raw() as jbyteArray;
    };
    match env.byte_array_from_slice(&encoded) {
        Ok(array) => array.into_raw(),
        Err(_) => JObject::null().into_raw() as jbyteArray,
    }
}

fn submit_java_frame(
    env: &mut JNIEnv<'_>,
    payload: JByteArray<'_>,
    kind: u8,
    pts: jlong,
    keyframe: bool,
) -> jint {
    let Ok(payload) = env.convert_byte_array(payload) else {
        return -3;
    };
    submit_media_frame(NativeMediaFrame {
        kind,
        pts,
        keyframe,
        payload,
    })
}

fn submit_media_frame(frame: NativeMediaFrame) -> jint {
    let Ok(mut state) = receiver_state().lock() else {
        return -1;
    };
    enqueue_media_frame(&mut state, frame)
}

fn enqueue_media_frame(state: &mut NativeReceiverState, frame: NativeMediaFrame) -> jint {
    if frame.kind != MEDIA_KIND_VIDEO && frame.kind != MEDIA_KIND_AUDIO {
        return -3;
    }
    if frame.payload.is_empty() || frame.payload.len() > MAX_JNI_MEDIA_FRAME_BYTES {
        return -3;
    }
    if state.session.is_none() {
        return -2;
    }
    if state.media_queue.len() >= MAX_JNI_MEDIA_QUEUE_FRAMES {
        return 1;
    }
    state.media_queue.push_back(frame);
    0
}

fn encode_media_frame(frame: &NativeMediaFrame) -> Vec<u8> {
    let mut output = Vec::with_capacity(19 + frame.payload.len());
    output.extend_from_slice(MEDIA_FRAME_MAGIC);
    output.push(MEDIA_FRAME_VERSION);
    output.push(frame.kind);
    output.push(u8::from(frame.keyframe));
    output.extend_from_slice(&frame.pts.to_be_bytes());
    output.extend_from_slice(&(frame.payload.len() as u32).to_be_bytes());
    output.extend_from_slice(&frame.payload);
    output
}

fn codec_wire_value(codec: MediaCodec) -> u8 {
    match codec {
        MediaCodec::H264 => 1,
        MediaCodec::H265 => 2,
        MediaCodec::Aac => 3,
        MediaCodec::Opus => 4,
        MediaCodec::Pcm => 5,
        MediaCodec::Alac => 6,
        MediaCodec::Unknown => 255,
    }
}

fn encode_video_config(config: VideoConfig) -> Vec<u8> {
    let mut output = Vec::with_capacity(24);
    output.extend_from_slice(MEDIA_CONFIG_MAGIC);
    output.extend_from_slice(&[
        MEDIA_CONFIG_VERSION,
        MEDIA_KIND_VIDEO,
        codec_wire_value(config.codec),
        0,
    ]);
    output.extend_from_slice(&config.width.to_be_bytes());
    output.extend_from_slice(&config.height.to_be_bytes());
    output.extend_from_slice(&config.frame_rate_numerator.to_be_bytes());
    output.extend_from_slice(&config.frame_rate_denominator.to_be_bytes());
    output
}

fn encode_audio_config(config: AudioConfig) -> Vec<u8> {
    let mut output = Vec::with_capacity(16);
    output.extend_from_slice(MEDIA_CONFIG_MAGIC);
    output.extend_from_slice(&[
        MEDIA_CONFIG_VERSION,
        MEDIA_KIND_AUDIO,
        codec_wire_value(config.codec),
        0,
    ]);
    output.extend_from_slice(&config.sample_rate.to_be_bytes());
    output.extend_from_slice(&u32::from(config.channels).to_be_bytes());
    output
}

fn set_video_config(config: VideoConfig) {
    if let Ok(mut state) = receiver_state().lock() {
        state.video_config = Some(config);
    }
}

fn set_audio_config(config: AudioConfig) {
    if let Ok(mut state) = receiver_state().lock() {
        state.audio_config = Some(config);
    }
}

fn take_video_config() -> Option<Vec<u8>> {
    receiver_state()
        .lock()
        .ok()
        .and_then(|mut state| state.video_config.take().map(encode_video_config))
}

fn take_audio_config() -> Option<Vec<u8>> {
    receiver_state()
        .lock()
        .ok()
        .and_then(|mut state| state.audio_config.take().map(encode_audio_config))
}

#[cfg(test)]
mod tests {
    use frameark_core::{
        AudioConfig, CORE_ABI_VERSION, DeviceId, MediaCodec, Session, SessionId, SessionState,
        TimeBase, TrackId, VideoConfig,
    };
    use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame};
    use frameark_native::{NativeSender, media_wire};
    use frameark_transport::{PairingClient, PairingCode};

    use super::*;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn ffi_uses_the_core_abi_version() {
        let _guard = test_guard();
        assert_eq!(CORE_ABI_VERSION, 1);
    }

    #[test]
    fn network_runtime_exposes_bounded_pairing_material_and_stops_cleanly() {
        let _guard = test_guard();
        let network = NetworkRuntime::start().unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(network.port, 0);
        assert!((0..=999_999).contains(&network.pairing_code));
        assert!(!network.certificate_der.is_empty());
        network.stop();
    }

    #[test]
    fn network_runtime_completes_loopback_pairing_and_media_delivery() {
        let _guard = test_guard();
        {
            let mut state = receiver_state().lock().unwrap();
            state.session = Some(Session::new(
                SessionId::try_from("loopback-receiver").unwrap(),
                DeviceId::try_from("loopback-device").unwrap(),
            ));
            state.media_queue.clear();
            state.network = None;
        }
        let network = NetworkRuntime::start().unwrap_or_else(|error| panic!("{error}"));
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), network.port);
        let pairing = PairingCode::parse(format!("{:06}", network.pairing_code))
            .expect("generated pairing code is six digits");
        let certificate = network.certificate_der.clone();
        std::thread::sleep(Duration::from_millis(50));
        let report = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("loopback runtime")
            .block_on(async {
                let transport = PairingClient::connect(
                    address,
                    "localhost",
                    &certificate,
                    pairing,
                    Duration::from_secs(5),
                )
                .await
                .expect("loopback FANP pairing");
                let mut sender = NativeSender::new(
                    transport,
                    Session::new(
                        SessionId::try_from("loopback-session").unwrap(),
                        DeviceId::try_from("loopback-sender").unwrap(),
                    ),
                )
                .unwrap();
                sender
                    .offer(
                        SessionOffer {
                            video: Some(VideoConfig {
                                codec: MediaCodec::H264,
                                width: 1280,
                                height: 720,
                                frame_rate_numerator: 30,
                                frame_rate_denominator: 1,
                            }),
                            audio: Some(AudioConfig {
                                codec: MediaCodec::Opus,
                                sample_rate: 48_000,
                                channels: 2,
                            }),
                            latency_ms: 120,
                        },
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                sender.start(Duration::from_secs(5)).await.unwrap();
                let time_base = TimeBase::new(90_000, 1).unwrap();
                let video_track = TrackId::new("video-0").unwrap();
                let audio_track = TrackId::new("audio-0").unwrap();
                let video = VideoFrame::new(
                    video_track,
                    1,
                    MediaTimestamp::new(0, time_base),
                    None,
                    true,
                    vec![0, 0, 0, 1, 0x65, 0x88],
                )
                .unwrap();
                let audio = AudioPacket::new(
                    audio_track,
                    1,
                    MediaTimestamp::new(0, TimeBase::new(48_000, 1).unwrap()),
                    960,
                    vec![0xF8, 0xFF, 0xFE],
                )
                .unwrap();
                let mut media = sender.open_media_stream().await.unwrap();
                media
                    .send_frame(
                        &media_wire::encode(&MediaPacket::Video(video)).unwrap(),
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                media
                    .send_frame(
                        &media_wire::encode(&MediaPacket::Audio(audio)).unwrap(),
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                media.finish().unwrap();
                sender.stop(Duration::from_secs(5)).await.unwrap()
            });
        assert_eq!(report.state, SessionState::Closed);
        network.stop();
        let mut state = receiver_state().lock().unwrap();
        assert_eq!(state.video_config.unwrap().width, 1280);
        assert_eq!(state.audio_config.unwrap().sample_rate, 48_000);
        assert_eq!(state.media_queue.len(), 2);
        assert_eq!(
            state.media_queue.pop_front().unwrap().kind,
            MEDIA_KIND_VIDEO
        );
        assert_eq!(
            state.media_queue.pop_front().unwrap().kind,
            MEDIA_KIND_AUDIO
        );
        state.session = None;
        state.media_queue.clear();
        state.network = None;
    }

    #[test]
    fn lifecycle_state_is_bounded_and_idempotent() {
        let _guard = test_guard();
        let state = receiver_state();
        let mut state = state.lock().unwrap();
        state.session = None;
        let session_id = SessionId::try_from("test-session").unwrap();
        let device_id = DeviceId::try_from("test-device").unwrap();
        let mut session = Session::new(session_id, device_id);
        session.transition(SessionState::Connecting).unwrap();
        assert_eq!(session.state(), SessionState::Connecting);
        session.transition(SessionState::Closed).unwrap();
        state.session = None;
        state.media_queue.clear();
    }

    #[test]
    fn media_queue_applies_backpressure_and_preserves_envelope() {
        let _guard = test_guard();
        let mut state = NativeReceiverState {
            session: Some(Session::new(
                SessionId::try_from("media-session").unwrap(),
                DeviceId::try_from("media-device").unwrap(),
            )),
            media_queue: VecDeque::new(),
            video_config: None,
            audio_config: None,
            network: None,
        };
        assert_eq!(
            enqueue_media_frame(
                &mut state,
                NativeMediaFrame {
                    kind: MEDIA_KIND_VIDEO,
                    pts: 90_000,
                    keyframe: true,
                    payload: vec![1, 2, 3],
                }
            ),
            0
        );
        let frame = state.media_queue.pop_front().unwrap();
        assert_eq!(
            encode_media_frame(&frame),
            [
                b'F', b'A', b'M', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 1, 95, 144, 0, 0, 0, 3, 1, 2, 3
            ]
        );
        for _ in 0..MAX_JNI_MEDIA_QUEUE_FRAMES {
            assert_eq!(
                enqueue_media_frame(
                    &mut state,
                    NativeMediaFrame {
                        kind: MEDIA_KIND_AUDIO,
                        pts: 0,
                        keyframe: false,
                        payload: vec![9],
                    }
                ),
                0
            );
        }
        assert_eq!(
            enqueue_media_frame(
                &mut state,
                NativeMediaFrame {
                    kind: MEDIA_KIND_AUDIO,
                    pts: 0,
                    keyframe: false,
                    payload: vec![9],
                }
            ),
            1
        );
        state.session = None;
        state.media_queue.clear();
    }

    #[test]
    fn jni_media_queue_sink_accepts_encoded_platform_samples() {
        let _guard = test_guard();
        let state = receiver_state();
        state.lock().unwrap().session = Some(Session::new(
            SessionId::try_from("sink-session").unwrap(),
            DeviceId::try_from("sink-device").unwrap(),
        ));
        let track_id = TrackId::new("video-0").unwrap();
        let mut sink = JniMediaQueueSink;
        sink.push_video(frameark_api::VideoFrame {
            track_id: &track_id,
            data: &[7, 8],
            timestamp: 90_000,
            keyframe: true,
        })
        .unwrap();
        let mut state = state.lock().unwrap();
        let queued = state.media_queue.pop_front().unwrap();
        assert_eq!(queued.kind, MEDIA_KIND_VIDEO);
        assert_eq!(queued.pts, 90_000);
        assert_eq!(queued.payload, vec![7, 8]);
        state.session = None;
        state.media_queue.clear();
    }
}
