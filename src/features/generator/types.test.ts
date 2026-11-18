import { describe, expect, it } from "vitest";
import {
  clampMinimums,
  DEFAULT_PASSPHRASE_OPTIONS,
  defaultLoginOptions,
  defaultPasswordOptions,
  hasSelectedCharset,
  LOGIN_LENGTH_PRESETS,
  LOGIN_MAX_LENGTH,
  MAX_EXCLUDE_CHARS,
  normalizeExcludeChars,
  PASSWORD_LENGTH_PRESETS,
  PASSWORD_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
} from "./types";

describe("generator options", () => {
  it("uses the documented defaults", () => {
    expect(defaultPasswordOptions.length).toBe(16);
    expect(defaultPasswordOptions.lowercase).toBe(true);
    expect(defaultPasswordOptions.uppercase).toBe(true);
    expect(defaultPasswordOptions.digits).toBe(true);
    expect(defaultPasswordOptions.symbols).toBe(true);
    expect(defaultPasswordOptions.excludeAmbiguous).toBe(true);
    expect(defaultPasswordOptions.minDigits).toBe(0);
    expect(defaultPasswordOptions.minSymbols).toBe(0);
    expect(defaultPasswordOptions.excludeChars).toBeNull();

    expect(defaultLoginOptions.length).toBe(12);
    expect(defaultLoginOptions.lowercase).toBe(true);
    expect(defaultLoginOptions.uppercase).toBe(false);
    expect(defaultLoginOptions.digits).toBe(true);
    expect(defaultLoginOptions.symbols).toBe(false);
    expect(defaultLoginOptions.minDigits).toBe(0);
    expect(defaultLoginOptions.minSymbols).toBe(0);

    expect(DEFAULT_PASSPHRASE_OPTIONS).toEqual({
      words: 5,
      separator: "-",
      capitalize: false,
      includeNumber: false,
      numberPosition: "end",
      wordlist: "eff",
    });
  });

  it("keeps the minimums inside what the backend accepts", () => {
    const clamped = clampMinimums({
      ...defaultPasswordOptions,
      length: 10,
      minDigits: 12,
      minSymbols: 4,
    });
    expect(clamped.minDigits).toBe(6);
    expect(clamped.minSymbols).toBe(4);

    const withoutDigits = clampMinimums({
      ...defaultPasswordOptions,
      digits: false,
      minDigits: 3,
    });
    expect(withoutDigits.minDigits).toBe(0);
  });

  it("detects an empty character set", () => {
    expect(
      hasSelectedCharset({
        ...defaultPasswordOptions,
        lowercase: false,
        uppercase: false,
        digits: false,
        symbols: false,
      }),
    ).toBe(false);
    expect(hasSelectedCharset({ ...defaultPasswordOptions, digits: true })).toBe(true);
  });

  it("keeps length presets inside the ranges the backend accepts", () => {
    for (const preset of PASSWORD_LENGTH_PRESETS) {
      expect(preset).toBeGreaterThanOrEqual(PASSWORD_MIN_LENGTH);
      expect(preset).toBeLessThanOrEqual(PASSWORD_MAX_LENGTH);
    }
    for (const preset of LOGIN_LENGTH_PRESETS) {
      expect(preset).toBeGreaterThanOrEqual(PASSWORD_MIN_LENGTH);
      expect(preset).toBeLessThanOrEqual(LOGIN_MAX_LENGTH);
    }
    expect(new Set(PASSWORD_LENGTH_PRESETS).size).toBe(PASSWORD_LENGTH_PRESETS.length);
    expect(new Set(LOGIN_LENGTH_PRESETS).size).toBe(LOGIN_LENGTH_PRESETS.length);
  });

  it("normalizes the excluded characters field", () => {
    expect(normalizeExcludeChars("")).toBeNull();
    expect(normalizeExcludeChars("   ")).toBeNull();
    expect(normalizeExcludeChars('<>"')).toBe('<>"');
    expect(normalizeExcludeChars(" < > ")).toBe("<>");
    expect(normalizeExcludeChars("a".repeat(MAX_EXCLUDE_CHARS + 10))).toHaveLength(
      MAX_EXCLUDE_CHARS,
    );
  });
});
