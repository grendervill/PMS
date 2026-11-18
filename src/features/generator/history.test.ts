import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearGeneratorHistory,
  GENERATOR_HISTORY_KEY,
  GENERATOR_HISTORY_LIMIT,
  pushGeneratorHistory,
  readGeneratorHistory,
  summarizeHistoryValue,
} from "./history";

function blockSessionStorage() {
  const spy = vi.spyOn(globalThis, "sessionStorage", "get");
  spy.mockImplementation(() => {
    throw new DOMException("blocked", "SecurityError");
  });
  return spy;
}

describe("generator history", () => {
  beforeEach(() => {
    globalThis.sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    globalThis.sessionStorage.clear();
  });

  it("stores the newest value first and reads it back", () => {
    pushGeneratorHistory({ value: "first-value", mode: "password" });
    const entries = pushGeneratorHistory({ value: "second-value", mode: "login" });

    expect(entries).toHaveLength(2);
    expect(entries[0]?.value).toBe("second-value");
    expect(entries[0]?.mode).toBe("login");
    expect(entries[1]?.value).toBe("first-value");
    expect(readGeneratorHistory()).toHaveLength(2);
    expect(globalThis.sessionStorage.getItem(GENERATOR_HISTORY_KEY)).toContain("second-value");
  });

  it("moves a repeated value up instead of duplicating it", () => {
    pushGeneratorHistory({ value: "alpha", mode: "password" });
    pushGeneratorHistory({ value: "beta", mode: "password" });
    const entries = pushGeneratorHistory({ value: "alpha", mode: "password" });

    expect(entries.map((entry) => entry.value)).toEqual(["alpha", "beta"]);
  });

  it("keeps the same value for different modes as separate rows", () => {
    pushGeneratorHistory({ value: "shared", mode: "password" });
    const entries = pushGeneratorHistory({ value: "shared", mode: "login" });

    expect(entries).toHaveLength(2);
  });

  it("keeps at most twenty entries", () => {
    for (let index = 0; index < GENERATOR_HISTORY_LIMIT + 5; index += 1) {
      pushGeneratorHistory({ value: `value-${index}`, mode: "password" });
    }

    const entries = readGeneratorHistory();
    expect(entries).toHaveLength(GENERATOR_HISTORY_LIMIT);
    expect(entries[0]?.value).toBe(`value-${GENERATOR_HISTORY_LIMIT + 4}`);
    expect(entries.at(-1)?.value).toBe("value-5");
  });

  it("clears everything on demand", () => {
    pushGeneratorHistory({ value: "secret", mode: "passphrase" });
    expect(readGeneratorHistory()).toHaveLength(1);

    clearGeneratorHistory();

    expect(readGeneratorHistory()).toEqual([]);
    expect(globalThis.sessionStorage.getItem(GENERATOR_HISTORY_KEY)).toBeNull();
  });

  it("ignores empty values", () => {
    expect(pushGeneratorHistory({ value: "", mode: "password" })).toEqual([]);
  });

  it("survives corrupted storage", () => {
    globalThis.sessionStorage.setItem(GENERATOR_HISTORY_KEY, "not json");
    expect(readGeneratorHistory()).toEqual([]);

    globalThis.sessionStorage.setItem(
      GENERATOR_HISTORY_KEY,
      JSON.stringify([{ value: 42 }, null, { mode: "password" }, { value: "ok", mode: "login" }]),
    );
    expect(readGeneratorHistory()).toEqual([{ value: "ok", mode: "login", at: 0 }]);

    globalThis.sessionStorage.setItem(GENERATOR_HISTORY_KEY, JSON.stringify({ value: "x" }));
    expect(readGeneratorHistory()).toEqual([]);
  });

  it("works when sessionStorage is blocked", () => {
    const spy = blockSessionStorage();

    expect(readGeneratorHistory()).toEqual([]);
    expect(pushGeneratorHistory({ value: "secret", mode: "password" })).toEqual([
      expect.objectContaining({ value: "secret" }),
    ]);
    expect(() => clearGeneratorHistory()).not.toThrow();

    spy.mockRestore();
  });

  it("shortens long values for the compact list", () => {
    expect(summarizeHistoryValue("short")).toBe("short");
    expect(summarizeHistoryValue("A7!kP9#vQ2$mR5@x")).toBe("A7!kP9#vQ2$…");
    expect(summarizeHistoryValue("абвгдеёжзийклмно")).toBe("абвгдеёжзий…");
  });
});
