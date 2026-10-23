#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "== macOS =="
pnpm tauri build

echo "== Android =="
: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK}"
export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
if [[ -z "${NDK_HOME:-}" ]]; then
  NDK_HOME="$(find "$ANDROID_HOME/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -1)"
fi
export NDK_HOME
if [[ -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
  export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
fi
export PATH="$HOME/.cargo/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"

pnpm tauri android build -t aarch64 --apk --ci

APK="$PWD/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"
KEYSTORE="${PMS_ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
if [[ -f "$KEYSTORE" && -x "$ANDROID_HOME/build-tools/36.0.0/apksigner" ]]; then
  echo "== Signing Android APK =="
  mkdir -p dist
  "$ANDROID_HOME/build-tools/36.0.0/apksigner" sign \
    --ks "$KEYSTORE" \
    --ks-pass "pass:${PMS_ANDROID_KEYSTORE_PASSWORD:-android}" \
    --key-pass "pass:${PMS_ANDROID_KEY_PASSWORD:-android}" \
    --out dist/PMS-android-release.apk "$APK"
  echo "APK: dist/PMS-android-release.apk"
fi
