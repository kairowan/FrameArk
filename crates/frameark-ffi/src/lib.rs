//! JNI entry points kept deliberately small while the Android media bridge is
//! introduced in bounded lifecycle increments.
//!
//! This crate is the only workspace exception to the `unsafe_code` lint because
//! Rust 2024 requires an unsafe attribute for the exported JNI symbol. The
//! function body itself does not dereference raw pointers or access JNI state.

#![allow(unsafe_code)]

use std::sync::{Mutex, OnceLock};

use frameark_core::{CORE_ABI_VERSION, DeviceId, Session, SessionId, SessionState};
use jni::{JNIEnv, objects::JObject, sys::jint};

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

#[cfg(test)]
mod tests {
    use frameark_core::{CORE_ABI_VERSION, DeviceId, Session, SessionId, SessionState};

    use super::receiver_state;

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
    }
}
