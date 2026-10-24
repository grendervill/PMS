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
RELEASE_DIR="${PMS_ANDROID_RELEASE_DIR:-$HOME/.config/pms/android-release}"
KEYSTORE="${PMS_ANDROID_KEYSTORE:-$RELEASE_DIR/pms-release.keystore}"
KEY_ALIAS="${PMS_ANDROID_KEY_ALIAS:-pms}"
KEYSTORE_PASSWORD="${PMS_ANDROID_KEYSTORE_PASSWORD:-}"
KEY_PASSWORD="${PMS_ANDROID_KEY_PASSWORD:-}"
if [[ -f "$RELEASE_DIR/pms-release.properties" ]]; then
  # shellcheck disable=SC1090
  source "$RELEASE_DIR/pms-release.properties"
  KEYSTORE_PASSWORD="${PMS_ANDROID_KEYSTORE_PASSWORD:-$storePassword}"
  KEY_PASSWORD="${PMS_ANDROID_KEY_PASSWORD:-$keyPassword}"
  KEY_ALIAS="${PMS_ANDROID_KEY_ALIAS:-$keyAlias}"
fi
KEY_PASSWORD="${KEY_PASSWORD:-$KEYSTORE_PASSWORD}"
if [[ ! -f "$KEYSTORE" ]]; then
  KEYSTORE="$HOME/.android/debug.keystore"
  KEY_ALIAS="${PMS_ANDROID_KEY_ALIAS:-androiddebugkey}"
  KEYSTORE_PASSWORD="${KEYSTORE_PASSWORD:-android}"
  KEY_PASSWORD="${KEY_PASSWORD:-android}"
fi
if [[ -f "$KEYSTORE" && -x "$ANDROID_HOME/build-tools/36.0.0/apksigner" ]]; then
  echo "== Signing Android APK =="
  VERSION="$(node -p "require('./package.json').version")"
  mkdir -p dist
  "$ANDROID_HOME/build-tools/36.0.0/apksigner" sign \
    --ks "$KEYSTORE" \
    --ks-key-alias "$KEY_ALIAS" \
    --ks-pass "pass:$KEYSTORE_PASSWORD" \
    --key-pass "pass:$KEY_PASSWORD" \
    --out "dist/PMS_${VERSION}_arm64.apk" "$APK"
  echo "APK: dist/PMS_${VERSION}_arm64.apk"
fi
