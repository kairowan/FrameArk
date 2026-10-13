//! Foreground reference sender: bounded commands, continuously driven QUIC,
//! cancellable setup/media, generation-scoped JNI calls, no stored credentials.
use super::*;
use frameark_core::{TimeBase, TrackId};
use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame as EncodedVideo};
use frameark_native::{NativeSender, media_wire};
use frameark_transport::{MAX_PAIRING_TICKET_BYTES, PairingClient, PairingTicket};
use jni::sys::jstring;
use std::sync::atomic::{AtomicI32, AtomicI64, AtomicUsize};
use std::sync::mpsc::{self, SyncSender, TryRecvError};

const MAX_QUEUE_BYTES: usize = 8 * 1024 * 1024;
const MAX_QUEUE_FRAMES: usize = 128;
const BUDGET: Duration = Duration::from_secs(5);
static NEXT_ID: AtomicI64 = AtomicI64::new(1);
static SENDER: Mutex<Option<Worker>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_version(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
) -> jint {
    CORE_ABI_VERSION as jint
}

struct Frame {
    payload: Vec<u8>,
    pts: i64,
    video: bool,
    keyframe: bool,
}
enum Command {
    Frame(Frame),
    Finish,
}
struct Worker {
    id: i64,
    commands: SyncSender<Command>,
    bytes: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    status: Arc<AtomicI32>,
    finishing: bool,
    join: JoinHandle<()>,
}

fn bounded_text(env: &mut JNIEnv<'_>, value: &JString<'_>, max: usize) -> Option<String> {
    // Check Java UTF-16 length before allocating its UTF-8 representation.
    let length = env
        .call_method(value, "length", "()I", &[])
        .ok()?
        .i()
        .ok()?;
    if length < 0 || length as usize > max {
        return None;
    }
    let text: String = env.get_string(value).ok()?.into();
    (text.len() <= max).then_some(text)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativePairingTicket(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    address: JString<'_>,
) -> jstring {
    let ticket = (|| {
        let ip = bounded_text(&mut env, &address, 15)?
            .parse::<Ipv4Addr>()
            .ok()?;
        let state = receiver_state().lock().ok()?;
        let network = state.network.as_ref()?;
        PairingTicket::new(
            std::net::SocketAddrV4::new(ip, network.port),
            network.certificate_der.clone(),
        )
        .ok()
    })();
    ticket
        .and_then(|ticket| env.new_string(ticket.encode()).ok())
        .map_or(std::ptr::null_mut(), |s| s.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeTicketSummary(
    env: JNIEnv<'_>,
    object: JObject<'_>,
    ticket: JString<'_>,
) -> jstring {
    Java_dev_frameark_sender_SenderNative_summary(env, object, ticket)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_summary(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    ticket: JString<'_>,
) -> jstring {
    bounded_text(&mut env, &ticket, MAX_PAIRING_TICKET_BYTES)
        .and_then(|text| PairingTicket::parse(&text).ok())
        .and_then(|ticket| {
            env.new_string(format!(
                "{}\nSHA-256 (TLS):\n{}",
                ticket.address(),
                ticket.fingerprint()
            ))
            .ok()
        })
        .map_or(std::ptr::null_mut(), |s| s.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_start(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    ticket: JString<'_>,
    pin: JString<'_>,
    width: jint,
    height: jint,
) -> jlong {
    let Some(ticket) = bounded_text(&mut env, &ticket, MAX_PAIRING_TICKET_BYTES)
        .and_then(|s| PairingTicket::parse(&s).ok())
    else {
        return -1;
    };
    let Some(pin) = bounded_text(&mut env, &pin, 6).and_then(|s| PairingCode::parse(s).ok()) else {
        return -1;
    };
    if !matches!((width, height), (1280, 720) | (1920, 1080)) {
        return -1;
    }
    let Ok(mut slot) = SENDER.lock() else {
        return -2;
    };
    if slot
        .as_ref()
        .is_some_and(|worker| !worker.join.is_finished())
    {
        return -2;
    }
    if let Some(previous) = slot.take() {
        let _ = previous.join.join();
    }
    match Worker::start(ticket, pin, width as u32, height as u32) {
        Ok(worker) => {
            let id = worker.id;
            *slot = Some(worker);
            id
        }
        Err(_) => -3,
    }
}

impl Worker {
    fn start(ticket: PairingTicket, pin: PairingCode, width: u32, height: u32) -> Result<Self, ()> {
        Self::start_at(
            ticket.address().into(),
            ticket.certificate().to_vec(),
            pin,
            width,
            height,
        )
    }

    // Production callers can reach this only through validated PairingTicket.
    fn start_at(
        address: SocketAddr,
        certificate: Vec<u8>,
        pin: PairingCode,
        width: u32,
        height: u32,
    ) -> Result<Self, ()> {
        let (commands, receiver) = mpsc::sync_channel(MAX_QUEUE_FRAMES);
        let bytes = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let status = Arc::new(AtomicI32::new(1));
        let (worker_bytes, worker_cancel, worker_status) =
            (bytes.clone(), cancel.clone(), status.clone());
        let join = thread::Builder::new()
            .name("frameark-sender".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| -10)
                    .and_then(|runtime| {
                        runtime.block_on(async {
                            until_stopped(
                                &worker_cancel,
                                tokio::time::timeout(Duration::from_secs(60), async {
                                    let transport = PairingClient::connect(
                                        address,
                                        NETWORK_SERVER_NAME,
                                        &certificate,
                                        pin,
                                        BUDGET,
                                    )
                                    .await
                                    .map_err(pairing_failure)?;
                                    let mut sender = NativeSender::new(
                                        transport,
                                        Session::new(
                                            SessionId::try_from("android-reference")
                                                .expect("static id"),
                                            DeviceId::try_from("android-reference-sender")
                                                .expect("static id"),
                                        ),
                                    )
                                    .map_err(|_| -12)?;
                                    sender
                                        .offer(
                                            SessionOffer {
                                                video: Some(VideoConfig {
                                                    codec: MediaCodec::H264,
                                                    width,
                                                    height,
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
                                        .map_err(|_| -13)?;
                                    sender.start(BUDGET).await.map_err(|_| -14)?;
                                    let mut stream =
                                        sender.open_media_stream().await.map_err(|_| -15)?;
                                    worker_status.store(2, Ordering::Release);
                                    let (mut video_sequence, mut audio_sequence) = (0, 0);
                                    let (mut last_video, mut last_audio) = (None, None);
                                    let mut last_command = Instant::now();
                                    loop {
                                        match receiver.try_recv() {
                                            Ok(Command::Frame(frame)) => {
                                                worker_bytes.fetch_sub(
                                                    frame.payload.len(),
                                                    Ordering::AcqRel,
                                                );
                                                let (sequence, last) = if frame.video {
                                                    (&mut video_sequence, &mut last_video)
                                                } else {
                                                    (&mut audio_sequence, &mut last_audio)
                                                };
                                                if last.is_some_and(|pts| frame.pts <= pts) {
                                                    return Err(-16);
                                                }
                                                *last = Some(frame.pts);
                                                let timestamp = MediaTimestamp::new(
                                                    frame.pts,
                                                    TimeBase::new(1, 1_000_000)
                                                        .expect("static timebase"),
                                                );
                                                let packet = if frame.video {
                                                    MediaPacket::Video(
                                                        EncodedVideo::new(
                                                            TrackId::new("video-0")
                                                                .expect("static track"),
                                                            *sequence,
                                                            timestamp,
                                                            None,
                                                            frame.keyframe,
                                                            frame.payload,
                                                        )
                                                        .map_err(|_| -16)?,
                                                    )
                                                } else {
                                                    MediaPacket::Audio(
                                                        AudioPacket::new(
                                                            TrackId::new("audio-0")
                                                                .expect("static track"),
                                                            *sequence,
                                                            timestamp,
                                                            21_333,
                                                            frame.payload,
                                                        )
                                                        .map_err(|_| -16)?,
                                                    )
                                                };
                                                *sequence += 1;
                                                stream
                                                    .send_frame(
                                                        &media_wire::encode(&packet)
                                                            .map_err(|_| -16)?,
                                                        BUDGET,
                                                    )
                                                    .await
                                                    .map_err(|_| -17)?;
                                                last_command = Instant::now();
                                            }
                                            Ok(Command::Finish) => break,
                                            Err(TryRecvError::Disconnected) => return Err(-18),
                                            Err(TryRecvError::Empty) => {
                                                if last_command.elapsed() > Duration::from_secs(10)
                                                {
                                                    return Err(-19);
                                                }
                                                tokio::time::sleep(Duration::from_millis(2)).await;
                                            }
                                        }
                                    }
                                    stream.finish().map_err(|_| -20)?;
                                    // Bounded grace for receiver playback; not an audible-output acknowledgment.
                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                    sender.stop(BUDGET).await.map_err(|_| -21)?;
                                    Ok(3)
                                }),
                            )
                            .await
                            .unwrap_or(Ok(Ok(4)))
                            .map_err(|_| -22)?
                        })
                    });
                drop(receiver);
                worker_status.store(result.unwrap_or_else(|code| code), Ordering::Release);
            })
            .map_err(|_| ())?;
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            commands,
            bytes,
            cancel,
            status,
            finishing: false,
            join,
        })
    }

    fn submit(&self, frame: Frame) -> i32 {
        if self.finishing
            || self.cancel.load(Ordering::Acquire)
            || self.status.load(Ordering::Acquire) != 2
        {
            return -1;
        }
        let size = frame.payload.len();
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(size)
                    .filter(|total| *total <= MAX_QUEUE_BYTES)
            })
            .is_err()
        {
            return 1;
        }
        match self.commands.try_send(Command::Frame(frame)) {
            Ok(()) => 0,
            Err(_) => {
                self.bytes.fetch_sub(size, Ordering::AcqRel);
                1
            }
        }
    }
}

fn pairing_failure(error: frameark_transport::TransportError) -> i32 {
    use frameark_transport::TransportError;
    match error {
        TransportError::Timeout => -110,
        TransportError::Endpoint | TransportError::EndpointClosed => -111,
        TransportError::TlsConfiguration => -112,
        TransportError::PairingRejected => -113,
        TransportError::ApprovalRejected => -114,
        TransportError::NegotiationRejected | TransportError::Negotiation(_) => -115,
        TransportError::ConnectionClosed => -116,
        _ => -117,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_submit(
    env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
    payload: JByteArray<'_>,
    pts: jlong,
    video: jboolean,
    keyframe: jboolean,
) -> jint {
    let max = if video != 0 {
        MAX_JNI_MEDIA_FRAME_BYTES
    } else {
        65_536
    };
    let Ok(length) = env.get_array_length(&payload) else {
        return -2;
    };
    if length <= 0 || length as usize > max || !(0..=31_000_000).contains(&pts) {
        return -2;
    }
    let Ok(slot) = SENDER.lock() else {
        return -1;
    };
    let Some(worker) = slot.as_ref().filter(|worker| worker.id == id) else {
        return -1;
    };
    let Ok(payload) = env.convert_byte_array(&payload) else {
        return -2;
    };
    worker.submit(Frame {
        payload,
        pts,
        video: video != 0,
        keyframe: keyframe != 0,
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_status(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) -> jint {
    SENDER
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .filter(|worker| worker.id == id)
                .map(|worker| worker.status.load(Ordering::Acquire))
        })
        .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_finish(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) -> jint {
    let Ok(mut slot) = SENDER.lock() else {
        return -1;
    };
    let Some(worker) = slot.as_mut().filter(|worker| worker.id == id) else {
        return -1;
    };
    if worker.finishing {
        return 0;
    }
    if worker.status.load(Ordering::Acquire) != 2 {
        return -1;
    }
    if worker.commands.try_send(Command::Finish).is_err() {
        return 1;
    }
    worker.finishing = true;
    0
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_cancel(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) {
    if let Ok(slot) = SENDER.lock()
        && let Some(worker) = slot.as_ref().filter(|worker| worker.id == id)
    {
        worker.cancel.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(size: usize) -> Frame {
        Frame {
            payload: vec![1; size],
            pts: 0,
            video: true,
            keyframe: true,
        }
    }

    #[test]
    fn queue_bounds_and_finish_cancel_gates() {
        let (commands, receiver) = mpsc::sync_channel(MAX_QUEUE_FRAMES);
        let mut worker = Worker {
            id: 1,
            commands,
            bytes: Arc::new(AtomicUsize::new(0)),
            cancel: Arc::new(AtomicBool::new(false)),
            status: Arc::new(AtomicI32::new(2)),
            finishing: false,
            join: thread::spawn(|| ()),
        };
        assert_eq!(worker.submit(frame(MAX_QUEUE_BYTES)), 0);
        assert_eq!(worker.submit(frame(1)), 1);
        drop(receiver.recv().unwrap());
        worker.bytes.store(0, Ordering::Release);
        for _ in 0..MAX_QUEUE_FRAMES {
            assert_eq!(worker.submit(frame(1)), 0);
        }
        assert_eq!(worker.submit(frame(1)), 1);
        assert_eq!(worker.bytes.load(Ordering::Acquire), MAX_QUEUE_FRAMES);
        worker.finishing = true;
        assert_eq!(worker.submit(frame(1)), -1);
        worker.finishing = false;
        worker.cancel.store(true, Ordering::Release);
        assert_eq!(worker.submit(frame(1)), -1);
        worker.cancel.store(false, Ordering::Release);
        drop(receiver);
        assert_eq!(worker.submit(frame(1)), 1);
        assert_eq!(worker.bytes.load(Ordering::Acquire), MAX_QUEUE_FRAMES);
        worker.join.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn setup_cancels_and_wrong_pin_fails_without_fallback() {
        let server = PairingServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            NETWORK_SERVER_NAME,
            PairingCode::parse("123456").unwrap(),
        )
        .unwrap();
        let worker = Worker::start_at(
            server.local_addr().unwrap(),
            server.certificate_der().to_vec(),
            PairingCode::parse("000000").unwrap(),
            1280,
            720,
        )
        .unwrap();
        assert!(server.accept_pairing(BUDGET).await.is_err());
        let deadline = Instant::now() + BUDGET;
        while !worker.join.is_finished() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(worker.join.is_finished());
        worker.join.join().unwrap();
        assert_eq!(worker.status.load(Ordering::Acquire), -113);
        // Listener accepts no second handshake; explicit cancellation must stop it promptly.
        let worker = Worker::start_at(
            server.local_addr().unwrap(),
            server.certificate_der().to_vec(),
            PairingCode::parse("123456").unwrap(),
            1280,
            720,
        )
        .unwrap();
        worker.cancel.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !worker.join.is_finished() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(worker.join.is_finished());
        worker.join.join().unwrap();
        assert_eq!(worker.status.load(Ordering::Acquire), 4);
        server.close();
    }
}
