import { useVaultStore } from "@/features/vault/store";

export const DEFAULT_CLIPBOARD_CLEAR_SECONDS = 30;

export const CLIPBOARD_CLEAR_OPTIONS = [
  { value: 5, label: "5 секунд" },
  { value: 10, label: "10 секунд" },
  { value: 20, label: "20 секунд" },
  { value: 30, label: "30 секунд" },
  { value: 45, label: "45 секунд" },
  { value: 60, label: "1 минута" },
  { value: 120, label: "2 минуты" },
  { value: 300, label: "5 минут" },
  { value: 600, label: "10 минут" },
  { value: 1800, label: "30 минут" },
  { value: 0, label: "Никогда" },
] as const;

export type ClipboardClearSeconds = (typeof CLIPBOARD_CLEAR_OPTIONS)[number]["value"];

export function resolveClipboardClearSeconds(seconds: number | undefined): number {
  if (seconds === undefined) return DEFAULT_CLIPBOARD_CLEAR_SECONDS;
  return CLIPBOARD_CLEAR_OPTIONS.some((option) => option.value === seconds)
    ? seconds
    : DEFAULT_CLIPBOARD_CLEAR_SECONDS;
}

export function clipboardClearLabel(seconds: number): string {
  return (
    CLIPBOARD_CLEAR_OPTIONS.find((option) => option.value === seconds)?.label ??
    `${seconds} секунд`
  );
}

const CLIPBOARD_CLEAR_IN_LABELS: Record<number, string> = {
  60: "1 минуту",
  120: "2 минуты",
  300: "5 минут",
  600: "10 минут",
  1800: "30 минут",
};

export function clipboardClearInLabel(seconds: number): string {
  return CLIPBOARD_CLEAR_IN_LABELS[seconds] ?? clipboardClearLabel(seconds).toLowerCase();
}

export function clipboardClearDelayMs(seconds: number): number {
  return seconds > 0 ? seconds * 1000 : 0;
}

/** Reads the timeout from the currently unlocked vault. */
export function currentClipboardClearSeconds(): number {
  return resolveClipboardClearSeconds(
    useVaultStore.getState().data?.settings.clipboardClearSeconds,
  );
}

export function clipboardClearToast(seconds: number): string {
  if (seconds <= 0) return "Автоочистка буфера обмена отключена.";
  return `Буфер обмена очистится через ${clipboardClearInLabel(seconds)}.`;
}
