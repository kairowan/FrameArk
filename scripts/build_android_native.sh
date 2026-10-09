#!/usr/bin/env bash
set -euo pipefail

readonly rust_toolchain="${FRAMEARK_RUST_TOOLCHAIN:-1.97.0}"
readonly target="${FRAMEARK_ANDROID_TARGET:-aarch64-linux-android}"
readonly abi="${FRAMEARK_ANDROID_ABI:-arm64-v8a}"
readonly api_level="${FRAMEARK_ANDROID_API_LEVEL:-26}"
readonly ndk_root="${ANDROID_NDK_LATEST_HOME:-${ANDROID_NDK_HOME:-}}"

if [[ -z "$ndk_root" ]]; then
  echo "Android NDK path is unavailable; set ANDROID_NDK_LATEST_HOME or ANDROID_NDK_HOME." >&2
  exit 1
fi

readonly toolchain_root="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64"
readonly linker="$toolchain_root/bin/${target}${api_level}-clang"
if [[ ! -x "$linker" ]]; then
  echo "Android NDK linker is unavailable: $linker" >&2
  exit 1
fi

rustup toolchain install "$rust_toolchain" --profile minimal
rustup target add "$target" --toolchain "$rust_toolchain"

target_key="${target^^}"
target_key="${target_key//-/_}"
export "CARGO_TARGET_${target_key}_LINKER=$linker"
rustup run "$rust_toolchain" cargo build \
  --locked \
  --release \
  --package frameark-ffi \
  --target "$target"

readonly output_dir="apps/android/receiver/src/main/jniLibs/$abi"
mkdir -p "$output_dir"
install -m 0644 \
  "target/$target/release/libframeark_ffi.so" \
  "$output_dir/libframeark_ffi.so"

echo "Built frameark-ffi for $target and installed it under $output_dir."
