import { describe, expect, it } from "vitest";
import { passwordStrength } from "./strength";

describe("password strength", () => {
  it("scores a common password as weak", async () => {
    const result = await passwordStrength("password");
    expect(result.score).toBeLessThanOrEqual(2);
  });

  it("scores a long passphrase as strong", async () => {
    const result = await passwordStrength("correct horse battery staple");
    expect(result.score).toBeGreaterThanOrEqual(3);
  });
});
