import { describe, expect, it } from "vitest";
import {
  CLIPBOARD_CLEAR_OPTIONS,
  clipboardClearDelayMs,
  clipboardClearInLabel,
  clipboardClearLabel,
  clipboardClearToast,
  resolveClipboardClearSeconds,
} from "./clipboardClear";

describe("clipboard clear settings", () => {
  it("offers the configured timeouts and never", () => {
    expect(CLIPBOARD_CLEAR_OPTIONS.map((option) => option.value)).toEqual([
      5, 10, 20, 30, 45, 60, 120, 300, 600, 1800, 0,
    ]);
    expect(CLIPBOARD_CLEAR_OPTIONS.at(-1)?.label).toBe("Никогда");
  });

  it("falls back to 30 seconds for legacy or unknown values", () => {
    expect(resolveClipboardClearSeconds(undefined)).toBe(30);
    expect(resolveClipboardClearSeconds(7)).toBe(30);
    expect(resolveClipboardClearSeconds(0)).toBe(0);
    expect(resolveClipboardClearSeconds(1800)).toBe(1800);
  });

  it("converts seconds to milliseconds and zero disables the timer", () => {
    expect(clipboardClearDelayMs(60)).toBe(60_000);
    expect(clipboardClearDelayMs(0)).toBe(0);
  });

  it("describes the selected timeout", () => {
    expect(clipboardClearLabel(60)).toBe("1 минута");
    expect(clipboardClearInLabel(60)).toBe("1 минуту");
    expect(clipboardClearToast(60)).toBe("Буфер обмена очистится через 1 минуту.");
    expect(clipboardClearToast(0)).toBe("Автоочистка буфера обмена отключена.");
  });
});
