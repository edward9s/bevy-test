#!/usr/bin/env bash
# Source this file after installing the pinned NDK; keep the build environment identical.
set -euo pipefail
export ANDROID_NDK_VERSION=28.2.13676358
export ANDROID_NDK_HOME="${ANDROID_HOME:?ANDROID_HOME is required}/ndk/$ANDROID_NDK_VERSION"
android_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$android_bin/aarch64-linux-android26-clang"
export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
export AR_aarch64_linux_android="$android_bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS='-Clink-arg=-Wl,-z,max-page-size=16384'
test -x "$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
if [[ -n "${GITHUB_ENV:-}" ]]; then
  for variable in ANDROID_NDK_VERSION ANDROID_NDK_HOME CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER CC_aarch64_linux_android AR_aarch64_linux_android CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS; do
    printf '%s=%s\n' "$variable" "${!variable}" >> "$GITHUB_ENV"
  done
fi
