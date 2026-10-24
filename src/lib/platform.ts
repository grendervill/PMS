export function isAndroidPlatform(): boolean {
  if (typeof window === "undefined") return false;
  const internals = (
    window as unknown as {
      __TAURI_ANDROID_FS_PLUGIN_INTERNALS__?: { isAndroid?: boolean };
    }
  ).__TAURI_ANDROID_FS_PLUGIN_INTERNALS__;
  if (typeof internals?.isAndroid === "boolean") return internals.isAndroid;
  return typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
}

export function isMacosPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Macintosh|Mac OS X/.test(navigator.userAgent) && !isAndroidPlatform();
}
