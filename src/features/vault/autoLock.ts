export type AutoLockOption =
  | "10s"
  | "20s"
  | "30s"
  | "2m"
  | "5m"
  | "10m"
  | "20m"
  | "30m"
  | "60m"
  | "quit"
  | "never";

export type AutoLockChoice = {
  value: AutoLockOption;
  label: string;
  description?: string;
};

export const AUTO_LOCK_OPTIONS: readonly AutoLockChoice[] = [
  { value: "10s", label: "10 секунд" },
  { value: "20s", label: "20 секунд" },
  { value: "30s", label: "30 секунд" },
  { value: "2m", label: "2 минуты" },
  { value: "5m", label: "5 минут" },
  { value: "10m", label: "10 минут" },
  { value: "20m", label: "20 минут" },
  { value: "30m", label: "30 минут" },
  { value: "60m", label: "60 минут" },
  {
    value: "quit",
    label: "До завершения программы",
    description:
      "Пока приложение работает, хранилище остаётся открытым. После закрытия или выгрузки из памяти потребуется вход.",
  },
  {
    value: "never",
    label: "Не блокировать",
    description:
      "Автоблокировка отключена. После перезапуска приложения вход всё равно потребуется.",
  },
];

const DURATIONS: Record<Exclude<AutoLockOption, "quit" | "never">, number> = {
  "10s": 10_000,
  "20s": 20_000,
  "30s": 30_000,
  "2m": 2 * 60_000,
  "5m": 5 * 60_000,
  "10m": 10 * 60_000,
  "20m": 20 * 60_000,
  "30m": 30 * 60_000,
  "60m": 60 * 60_000,
};

export function autoLockTimeoutMs(option: AutoLockOption): number | null {
  if (option === "quit" || option === "never") return null;
  return DURATIONS[option];
}

export function autoLockLabel(option: AutoLockOption): string {
  return AUTO_LOCK_OPTIONS.find((choice) => choice.value === option)?.label ?? "5 минут";
}

export function legacyAutoLockOption(minutes: number): AutoLockOption {
  if (minutes <= 0) return "never";
  const exact: Record<number, AutoLockOption> = {
    2: "2m",
    5: "5m",
    10: "10m",
    20: "20m",
    30: "30m",
    60: "60m",
  };
  return exact[minutes] ?? "5m";
}

export function resolveAutoLockOption(settings: {
  autoLock?: AutoLockOption;
  autoLockMinutes?: number;
}): AutoLockOption {
  return settings.autoLock ?? legacyAutoLockOption(settings.autoLockMinutes ?? 5);
}
