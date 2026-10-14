//! Real QUIC loss, not a mock reconnect or a physical Wi-Fi acceptance test.
use frameark_api::{AudioFrame, AudioRenderer, VideoFrame, VideoRenderer};
use frameark_core::{
    AudioConfig, DeviceId, ErrorKind, MediaCodec, Session, SessionId, SessionState, TimeBase,
    TrackId, VideoConfig,
};
use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame as EncodedVideo};
use frameark_native::{
    MediaRendererFactory, NativeMediaControlReceiver, NativeMediaSessionReport, NativeSender,
    ReceiverPolicy, SessionBackend, SessionOffer, media_session::MediaSession, media_wire,
};
use frameark_transport::{MediaSender, PairingClient, PairingCode, PairingServer};
use std::net::SocketAddr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::{net::UdpSocket, task::JoinHandle, time::timeout};

const BUDGET: Duration = Duration::from_secs(5);

/// Single loopback peer only. Drops datagrams instead of delaying application
/// writes, so TLS, retransmission, framing and application deadlines are real.
struct LossProxy {
    address: SocketAddr,
    blocked: Arc<AtomicBool>,
    dropped: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl LossProxy {
    async fn start(server: SocketAddr) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let blocked = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicUsize::new(0));
        let block = blocked.clone();
        let count = dropped.clone();
        let task = tokio::spawn(async move {
            let mut client = None;
            let mut bytes = vec![0; 65_535];
            loop {
                let (len, from) = socket.recv_from(&mut bytes).await.unwrap();
                let destination = if from == server {
                    let Some(client) = client else { continue };
                    client
                } else {
                    let peer = *client.get_or_insert(from);
                    if from != peer {
                        continue;
                    }
                    server
                };
                if block.load(Ordering::SeqCst) {
                    count.fetch_add(1, Ordering::SeqCst);
                } else {
                    socket.send_to(&bytes[..len], destination).await.unwrap();
                }
            }
        });
        Self {
            address,
            blocked,
            dropped,
            task,
        }
    }

    fn block(&self, value: bool) {
        self.blocked.store(value, Ordering::SeqCst);
    }
}

impl Drop for LossProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
struct Calls {
    backend: Vec<&'static str>,
    frames: Vec<(u8, i64, Vec<u8>)>,
    resets: usize,
}

struct Backend(Arc<Mutex<Calls>>);
impl SessionBackend for Backend {
    fn prepare(&mut self, _: SessionOffer) -> frameark_core::Result<()> {
        self.0.lock().unwrap().backend.push("prepare");
        Ok(())
    }
    fn start(&mut self) -> frameark_core::Result<()> {
        self.0.lock().unwrap().backend.push("start");
        Ok(())
    }
    fn reset(&mut self) -> frameark_core::Result<()> {
        self.0.lock().unwrap().backend.push("reset");
        Ok(())
    }
}

struct Video(Arc<Mutex<Calls>>);
impl VideoRenderer for Video {
    type Error = ();
    fn configure(&mut self, _: VideoConfig) -> Result<(), ()> {
        Ok(())
    }
    fn render(&mut self, frame: VideoFrame<'_>) -> Result<(), ()> {
        self.0
            .lock()
            .unwrap()
            .frames
            .push((1, frame.timestamp, frame.data.to_vec()));
        Ok(())
    }
    fn reset(&mut self) -> Result<(), ()> {
        self.0.lock().unwrap().resets += 1;
        Ok(())
    }
}

struct Audio(Arc<Mutex<Calls>>);
impl AudioRenderer for Audio {
    type Error = ();
    fn configure(&mut self, _: AudioConfig) -> Result<(), ()> {
        Ok(())
    }
    fn render(&mut self, frame: AudioFrame<'_>) -> Result<(), ()> {
        self.0
            .lock()
            .unwrap()
            .frames
            .push((2, frame.timestamp, frame.data.to_vec()));
        Ok(())
    }
    fn reset(&mut self) -> Result<(), ()> {
        self.0.lock().unwrap().resets += 1;
        Ok(())
    }
}

struct Factory(Arc<Mutex<Calls>>);
impl MediaRendererFactory for Factory {
    type Video = Video;
    type Audio = Audio;
    fn prepare(
        &mut self,
        offer: SessionOffer,
    ) -> frameark_core::Result<MediaSession<Video, Audio>> {
        MediaSession::prepare(
            offer,
            Some((TrackId::new("video-0").unwrap(), Video(self.0.clone()))),
            Some((TrackId::new("audio-0").unwrap(), Audio(self.0.clone()))),
        )
    }
}

fn session(id: &str) -> Session {
    Session::new(
        SessionId::try_from(id).unwrap(),
        DeviceId::try_from("loss-test").unwrap(),
    )
}

struct Running {
    server: Arc<PairingServer>,
    proxy: LossProxy,
    calls: Arc<Mutex<Calls>>,
    receiver: JoinHandle<frameark_core::Result<NativeMediaSessionReport>>,
    sender: NativeSender,
    stream: MediaSender,
}

async fn start(receiver_budget: Duration) -> Running {
    let server = Arc::new(
        PairingServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            "localhost",
            PairingCode::parse("123456").unwrap(),
        )
        .unwrap(),
    );
    let proxy = LossProxy::start(server.local_addr().unwrap()).await;
    let accepting = server.clone();
    let calls = Arc::new(Mutex::new(Calls::default()));
    let receiver_calls = calls.clone();
    let receiver = tokio::spawn(async move {
        let transport = accepting.accept_pairing(BUDGET).await.unwrap();
        NativeMediaControlReceiver::new(
            transport,
            session("loss-receiver"),
            ReceiverPolicy {
                video: true,
                opus: false,
                aac: true,
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
                max_channels: 2,
            },
            Backend(receiver_calls.clone()),
            Factory(receiver_calls),
        )
        .unwrap()
        .serve(receiver_budget)
        .await
    });
    let transport = PairingClient::connect(
        proxy.address,
        "localhost",
        server.certificate_der(),
        PairingCode::parse("123456").unwrap(),
        BUDGET,
    )
    .await
    .unwrap();
    let mut sender = NativeSender::new(transport, session("loss-sender")).unwrap();
    sender
        .offer(
            SessionOffer {
                video: Some(VideoConfig {
                    codec: MediaCodec::H264,
                    width: 1920,
                    height: 1080,
                    frame_rate_numerator: 30,
                    frame_rate_denominator: 1,
                }),
                audio: Some(AudioConfig {
                    codec: MediaCodec::Aac,
                    sample_rate: 48_000,
                    channels: 2,
                }),
                latency_ms: 120,
            },
            BUDGET,
        )
        .await
        .unwrap();
    sender.start(BUDGET).await.unwrap();
    let stream = sender.open_media_stream().await.unwrap();
    Running {
        server,
        proxy,
        calls,
        receiver,
        sender,
        stream,
    }
}

async fn send_pair(stream: &mut MediaSender, index: u8) {
    let stamp = MediaTimestamp::new(i64::from(index) * 3_000, TimeBase::new(1, 90_000).unwrap());
    // Opaque bounded payloads deliberately do not claim codec decode evidence.
    let video = MediaPacket::Video(
        EncodedVideo::new(
            TrackId::new("video-0").unwrap(),
            u64::from(index),
            stamp,
            None,
            true,
            vec![index; 16_384],
        )
        .unwrap(),
    );
    let audio = MediaPacket::Audio(
        AudioPacket::new(
            TrackId::new("audio-0").unwrap(),
            u64::from(index),
            stamp,
            1024,
            vec![index; 256],
        )
        .unwrap(),
    );
    for packet in [video, audio] {
        stream
            .send_frame(&media_wire::encode(&packet).unwrap(), BUDGET)
            .await
            .unwrap();
    }
}

async fn wait_frames(calls: &Mutex<Calls>, expected: usize) {
    timeout(BUDGET, async {
        while calls.lock().unwrap().frames.len() < expected {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("frames reached the renderer boundary");
}

fn assert_released(calls: &Mutex<Calls>) {
    let calls = calls.lock().unwrap();
    assert_eq!(calls.backend, ["prepare", "start", "reset"]);
    assert_eq!(calls.resets, 2, "each output is reset exactly once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_authenticated_stream_survives_two_one_second_blackouts() {
    let mut run = start(BUDGET).await;
    let certificate = run.server.certificate_der().to_vec();
    let address = run.server.local_addr().unwrap();
    send_pair(&mut run.stream, 0).await;
    wait_frames(&run.calls, 2).await;
    for index in 1..=2 {
        let before = run.proxy.dropped.load(Ordering::SeqCst);
        run.proxy.block(true);
        send_pair(&mut run.stream, index).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(
            run.proxy.dropped.load(Ordering::SeqCst) > before,
            "actual UDP was lost"
        );
        assert_eq!(
            run.calls.lock().unwrap().frames.len(),
            usize::from(index) * 2
        );
        assert_eq!(run.calls.lock().unwrap().resets, 0);
        run.proxy.block(false);
        wait_frames(&run.calls, usize::from(index + 1) * 2).await;
        assert_eq!(run.sender.state(), SessionState::Streaming);
    }
    send_pair(&mut run.stream, 3).await;
    wait_frames(&run.calls, 8).await;
    run.stream.finish().unwrap();
    assert_eq!(
        run.sender.stop(BUDGET).await.unwrap().state,
        SessionState::Closed
    );
    let report = timeout(BUDGET, run.receiver)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        report.control.requests, 3,
        "one Offer/Start/Stop, no second authentication"
    );
    assert_eq!(report.media.video_frames, 4);
    assert_eq!(report.media.audio_frames, 4);
    let expected: Vec<_> = (0..4_u8)
        .flat_map(|index| {
            [
                (1, i64::from(index) * 3_000, vec![index; 16_384]),
                (2, i64::from(index) * 3_000, vec![index; 256]),
            ]
        })
        .collect();
    assert_eq!(
        run.calls.lock().unwrap().frames,
        expected,
        "no duplicates, loss or timestamp mutation"
    );
    assert_eq!(run.server.certificate_der(), certificate);
    assert_eq!(run.server.local_addr().unwrap(), address);
    assert_released(&run.calls);
    run.server.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blackout_past_media_deadline_releases_session_without_retry() {
    let mut run = start(Duration::from_secs(2)).await;
    send_pair(&mut run.stream, 0).await;
    wait_frames(&run.calls, 2).await;
    run.proxy.block(true);
    send_pair(&mut run.stream, 1).await;
    let error = timeout(BUDGET, run.receiver)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Timeout);
    assert!(run.proxy.dropped.load(Ordering::SeqCst) > 0);
    assert_eq!(run.calls.lock().unwrap().frames.len(), 2);
    assert_released(&run.calls);
    run.sender.abort();
    assert_eq!(run.sender.state(), SessionState::Closed);
    run.server.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_blackout_drops_both_outputs_without_waiting_for_network() {
    let mut run = start(BUDGET).await;
    send_pair(&mut run.stream, 0).await;
    wait_frames(&run.calls, 2).await;
    run.proxy.block(true);
    run.receiver.abort();
    assert!(
        timeout(Duration::from_secs(1), run.receiver)
            .await
            .unwrap()
            .unwrap_err()
            .is_cancelled()
    );
    assert_released(&run.calls);
    run.sender.abort();
    assert_eq!(run.sender.state(), SessionState::Closed);
    run.server.close();
}
