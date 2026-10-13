//! Opt-in instrumentation sender. No symbol is present in normal release builds.
use super::*;
use frameark_core::{TimeBase, TrackId};
use frameark_media::{AudioPacket, MediaPacket, MediaTimestamp, VideoFrame as EncodedVideo};
use frameark_native::{NativeSender, media_wire};
use frameark_transport::PairingClient;

static LAST_FAILURE: Mutex<String> = Mutex::new(String::new());

pub(super) fn record_failure(code: &str) {
    if let Ok(mut slot) = LAST_FAILURE.lock()
        && (code.is_empty() || slot.is_empty())
    {
        *slot = code.chars().take(96).collect();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_PlaybackNetworkProbe_diagnostic(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jni::sys::jstring {
    let code = LAST_FAILURE.lock().map(|s| s.clone()).unwrap_or_default();
    env.new_string(code)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_PlaybackNetworkProbe_send(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    video: JByteArray<'_>,
    audio: JByteArray<'_>,
) -> jint {
    let run = || -> Result<(), i32> {
        record_failure("");
        if !(1..=1_048_576).contains(&env.get_array_length(&video).map_err(|_| -1)?)
            || !(1..=65_536).contains(&env.get_array_length(&audio).map_err(|_| -1)?)
        {
            return Err(-1);
        }
        let video = env.convert_byte_array(&video).map_err(|_| -1)?;
        let audio = env.convert_byte_array(&audio).map_err(|_| -1)?;
        let (port, pin, certificate) = {
            let state = receiver_state().lock().map_err(|_| -2)?;
            let network = state.network.as_ref().ok_or(-2)?;
            (
                network.port,
                network.pairing_code,
                network.certificate_der.clone(),
            )
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| -3)?;
        runtime.block_on(async {
            let budget = Duration::from_secs(5);
            let transport = PairingClient::connect(
                SocketAddr::from(([127, 0, 0, 1], port)),
                NETWORK_SERVER_NAME,
                &certificate,
                PairingCode::parse(format!("{pin:06}")).map_err(|_| -4)?,
                budget,
            )
            .await
            .map_err(|_| -10)?;
            let mut sender = NativeSender::new(
                transport,
                Session::new(
                    SessionId::try_from("android-probe").map_err(|_| -11)?,
                    DeviceId::try_from("android-probe-sender").map_err(|_| -11)?,
                ),
            )
            .map_err(|_| -12)?;
            sender
                .offer(
                    SessionOffer {
                        video: Some(VideoConfig {
                            codec: MediaCodec::H264,
                            width: 320,
                            height: 240,
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
                    budget,
                )
                .await
                .map_err(|_| -20)?;
            sender.start(budget).await.map_err(|_| -30)?;
            let mut stream = sender.open_media_stream().await.map_err(|_| -40)?;
            let started = tokio::time::Instant::now();
            let mut audio_index = 0_u64;
            for index in 0..90_u64 {
                tokio::time::sleep_until(started + Duration::from_micros(index * 1_000_000 / 30))
                    .await;
                let frame = MediaPacket::Video(
                    EncodedVideo::new(
                        TrackId::new("video-0").map_err(|_| -41)?,
                        index,
                        MediaTimestamp::new(
                            (index * 3_000) as i64,
                            TimeBase::new(1, 90_000).map_err(|_| -41)?,
                        ),
                        None,
                        true,
                        video.clone(),
                    )
                    .map_err(|_| -41)?,
                );
                stream
                    .send_frame(&media_wire::encode(&frame).map_err(|_| -42)?, budget)
                    .await
                    .map_err(|_| -43)?;
                while audio_index * 1024 <= index * 1600 {
                    let frame = MediaPacket::Audio(
                        AudioPacket::new(
                            TrackId::new("audio-0").map_err(|_| -44)?,
                            audio_index,
                            MediaTimestamp::new(
                                (audio_index * 1024) as i64,
                                TimeBase::new(1, 48_000).map_err(|_| -44)?,
                            ),
                            1024,
                            audio.clone(),
                        )
                        .map_err(|_| -44)?,
                    );
                    stream
                        .send_frame(&media_wire::encode(&frame).map_err(|_| -45)?, budget)
                        .await
                        .map_err(|_| -46)?;
                    audio_index += 1;
                }
            }
            stream.finish().map_err(|_| -50)?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            sender.stop(budget).await.map_err(|_| -60)?;
            Ok(())
        })
    };
    run().err().unwrap_or(0)
}
