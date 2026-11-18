import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { isAndroidPlatform } from "@/lib/platform";

let clearTimer: ReturnType<typeof setTimeout> | null = null;

export type CopyOptions = {
  clearAfterMs?: number;
};

/**
 * Writes a secret to the clipboard, asking the OS not to keep a copy in
 * clipboard history or iCloud/Universal Clipboard (where supported: macOS marks
 * the item as concealed/transient, other platforms fall back to a plain copy).
 */
async function writeSecret(value: string): Promise<void> {
  if (isAndroidPlatform()) {
    await writeText(value);
    return;
  }
  try {
    await invoke("clipboard_copy_secret", { value });
  } catch {
    // Older builds without the command, or a platform without marking.
    await writeText(value);
  }
}

/**
 * Copies a secret and best-effort clears the clipboard later only if the
 * clipboard still contains our value. A non-positive timeout keeps the value
 * until the user replaces it. Returns a cleanup function.
 */
export async function copySecret(
  value: string,
  options: CopyOptions = {},
): Promise<() => void> {
  if (!value) return () => undefined;

  if (clearTimer) {
    clearTimeout(clearTimer);
    clearTimer = null;
  }

  await writeSecret(value);

  const delay = options.clearAfterMs ?? 30_000;
  if (delay <= 0) return () => undefined;

  const timer = setTimeout(() => {
    if (clearTimer === timer) clearTimer = null;
    void (async () => {
      try {
        const current = await readText();
        if (current === value) {
          await writeText("");
        }
      } catch {
        // Clipboard may be unavailable; clearing is best-effort.
      }
    })();
  }, delay);
  clearTimer = timer;

  return () => {
    if (clearTimer === timer) {
      clearTimeout(timer);
      clearTimer = null;
    }
  };
}
