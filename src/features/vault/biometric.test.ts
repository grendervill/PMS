import { beforeEach, describe, expect, it } from "vitest";
import { isBiometricEnabled, setBiometricEnabled } from "./biometric";

describe("biometric local flag", () => {
  beforeEach(() => localStorage.clear());

  it("stores the flag per vault", () => {
    expect(isBiometricEnabled("a")).toBe(false);
    setBiometricEnabled("a", true);
    expect(isBiometricEnabled("a")).toBe(true);
    expect(isBiometricEnabled("b")).toBe(false);
    setBiometricEnabled("a", false);
    expect(isBiometricEnabled("a")).toBe(false);
  });
});
