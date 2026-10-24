import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";

let clearTimer: ReturnType<typeof setTimeout> | null = null;

export type CopyOptions = {
  clearAfterMs?: number;
};

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

  await writeText(value);

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
