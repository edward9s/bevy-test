#!/usr/bin/env bash
set -euo pipefail
for name in KEYSTORE_BASE64 STORE_PASSWORD KEY_ALIAS KEY_PASSWORD; do
  if [[ -z "${!name:-}" ]]; then echo "::error::Missing repository secret: $name"; exit 1; fi
done
build_tools="$ANDROID_HOME/build-tools/35.0.0"
ndk_tools="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
mkdir -p build/package/lib/arm64-v8a
cp target/aarch64-linux-android/release/libbevy_forest.so build/package/lib/arm64-v8a/
# Cargo strips DWARF only; remove static symbol tables from the packaged copy.
"$ndk_tools/llvm-strip" --strip-unneeded build/package/lib/arm64-v8a/libbevy_forest.so
"$ndk_tools/llvm-nm" -D --defined-only \
  build/package/lib/arm64-v8a/libbevy_forest.so | awk '{print $NF}' > build/exports.txt
grep -Fx 'ANativeActivity_onCreate' build/exports.txt
"$build_tools/aapt" package -f -M android/AndroidManifest.xml \
  -I "$ANDROID_HOME/platforms/android-35/android.jar" -A assets -F build/unsigned.apk
# Native libraries are extracted at install time (extractNativeLibs=true).
(cd build/package && zip -q -9 ../unsigned.apk lib/arm64-v8a/libbevy_forest.so)
"$build_tools/zipalign" -P 16 -f 4 build/unsigned.apk build/aligned.apk
umask 077
key="${RUNNER_TEMP:-/tmp}/bevy-release.jks"
trap 'rm -f "$key"' EXIT
printf '%s' "$KEYSTORE_BASE64" | tr -d '[:space:]' | base64 --decode > "$key"
"$build_tools/apksigner" sign --ks "$key" --ks-key-alias "$KEY_ALIAS" \
  --ks-pass env:STORE_PASSWORD --key-pass env:KEY_PASSWORD \
  --out build/BevyForest-test-arm64.apk build/aligned.apk
"$build_tools/apksigner" verify --verbose build/BevyForest-test-arm64.apk
"$build_tools/zipalign" -c -P 16 4 build/BevyForest-test-arm64.apk
unzip -l build/BevyForest-test-arm64.apk > build/apk-contents.txt
grep -F 'assets/generated/forest.glb' build/apk-contents.txt
