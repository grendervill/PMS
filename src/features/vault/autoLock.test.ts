import { describe, expect, it } from "vitest";
import {
  AUTO_LOCK_OPTIONS,
  autoLockLabel,
  autoLockTimeoutMs,
  legacyAutoLockOption,
  resolveAutoLockOption,
} from "./autoLock";

describe("auto-lock options", () => {
  it("exposes every requested duration and special mode", () => {
    expect(AUTO_LOCK_OPTIONS.map((option) => option.value)).toEqual([
      "10s",
      "20s",
      "30s",
      "2m",
      "5m",
      "10m",
      "20m",
      "30m",
      "60m",
      "quit",
      "never",
    ]);
  });

  it("maps durations to milliseconds", () => {
    expect(autoLockTimeoutMs("10s")).toBe(10_000);
    expect(autoLockTimeoutMs("30s")).toBe(30_000);
    expect(autoLockTimeoutMs("2m")).toBe(120_000);
    expect(autoLockTimeoutMs("60m")).toBe(3_600_000);
  });

  it("disables inactivity locking for quit and never", () => {
    expect(autoLockTimeoutMs("quit")).toBeNull();
    expect(autoLockTimeoutMs("never")).toBeNull();
  });

  it("labels options in Russian", () => {
    expect(autoLockLabel("10s")).toBe("10 секунд");
    expect(autoLockLabel("2m")).toBe("2 минуты");
    expect(autoLockLabel("quit")).toBe("До завершения программы");
    expect(autoLockLabel("never")).toBe("Не блокировать");
  });

  it("migrates legacy minute settings", () => {
    expect(legacyAutoLockOption(5)).toBe("5m");
    expect(legacyAutoLockOption(2)).toBe("2m");
    expect(legacyAutoLockOption(0)).toBe("never");
    expect(legacyAutoLockOption(1)).toBe("5m");
  });

  it("prefers the new option when present and falls back for old vaults", () => {
    expect(resolveAutoLockOption({ autoLock: "20s", autoLockMinutes: 5 })).toBe("20s");
    expect(resolveAutoLockOption({ autoLockMinutes: 10 })).toBe("10m");
    expect(resolveAutoLockOption({})).toBe("5m");
  });
});
