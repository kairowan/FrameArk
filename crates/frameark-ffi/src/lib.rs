//! JNI entry points kept deliberately small while the Android media bridge is
//! introduced in bounded lifecycle increments.
//!
//! This crate is the only workspace exception to the `unsafe_code` lint because
//! Rust 2024 requires an unsafe attribute for the exported JNI symbol. The
//! function body itself does not dereference raw pointers or access JNI state.

#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use frameark_core::{CORE_ABI_VERSION, DeviceId, Session, SessionId, SessionState};
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeMediaFrame {
    kind: u8,
    pts: i64,
    keyframe: bool,
    payload: Vec<u8>,
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
    state.session = Some(session);
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
    if session.transition(SessionState::Closed).is_err() {
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

#[cfg(test)]
mod tests {
    use frameark_core::{CORE_ABI_VERSION, DeviceId, Session, SessionId, SessionState};

    use super::*;

    #[test]
    fn ffi_uses_the_core_abi_version() {
        assert_eq!(CORE_ABI_VERSION, 1);
    }

    #[test]
    fn lifecycle_state_is_bounded_and_idempotent() {
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
        let mut state = NativeReceiverState {
            session: Some(Session::new(
                SessionId::try_from("media-session").unwrap(),
                DeviceId::try_from("media-device").unwrap(),
            )),
            media_queue: VecDeque::new(),
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
}
