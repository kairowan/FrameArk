//! JNI entry points kept deliberately small until the Android media bridge is defined.
//!
//! This crate is the only workspace exception to the `unsafe_code` lint because
//! Rust 2024 requires an unsafe attribute for the exported JNI symbol. The
//! function body itself does not dereference raw pointers or access JNI state.

#![allow(unsafe_code)]

use frameark_core::CORE_ABI_VERSION;
use jni::{JNIEnv, objects::JObject, sys::jint};

/// Returns the core ABI version expected by the Android receiver.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_FrameArkNative_nativeVersion(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jint {
    CORE_ABI_VERSION as jint
}

#[cfg(test)]
mod tests {
    use frameark_core::CORE_ABI_VERSION;

    #[test]
    fn ffi_uses_the_core_abi_version() {
        assert_eq!(CORE_ABI_VERSION, 1);
    }
}
