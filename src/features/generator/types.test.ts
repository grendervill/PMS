import { describe, expect, it } from "vitest";
import {
  defaultLoginOptions,
  defaultPasswordOptions,
  hasSelectedCharset,
} from "./types";

describe("generator options", () => {
  it("uses the documented defaults", () => {
    expect(defaultPasswordOptions.length).toBe(16);
    expect(defaultPasswordOptions.lowercase).toBe(true);
    expect(defaultPasswordOptions.uppercase).toBe(true);
    expect(defaultPasswordOptions.digits).toBe(true);
    expect(defaultPasswordOptions.symbols).toBe(true);
    expect(defaultPasswordOptions.excludeAmbiguous).toBe(true);

    expect(defaultLoginOptions.length).toBe(12);
    expect(defaultLoginOptions.lowercase).toBe(true);
    expect(defaultLoginOptions.uppercase).toBe(false);
    expect(defaultLoginOptions.digits).toBe(true);
    expect(defaultLoginOptions.symbols).toBe(false);
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
});
