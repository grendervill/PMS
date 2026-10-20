import { describe, expect, it } from "vitest";
import { fromBase64, toBase64 } from "./base64";

describe("base64", () => {
  it("roundtrips arbitrary bytes", () => {
    const bytes = new Uint8Array(70_000);
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = index % 251;
    }

    expect(fromBase64(toBase64(bytes))).toEqual(bytes);
  });

  it("handles empty input", () => {
    expect(toBase64(new Uint8Array())).toBe("");
    expect(fromBase64("")).toEqual(new Uint8Array());
  });
});
