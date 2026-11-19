import { beforeEach, describe, expect, it } from "vitest";
import { clearMode, getMode, setMode } from "./mode";

describe("storage mode", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("is unset on a fresh install", () => {
    expect(getMode()).toBeNull();
  });

  it("remembers the chosen mode", () => {
    setMode("remote");
    expect(getMode()).toBe("remote");
    setMode("local");
    expect(getMode()).toBe("local");
    clearMode();
    expect(getMode()).toBeNull();
  });

  it("ignores garbage instead of guessing", () => {
    localStorage.setItem("pms.storageMode.v1", "cloud");
    expect(getMode()).toBeNull();
    localStorage.setItem("pms.storageMode.v1", JSON.stringify({ mode: "remote" }));
    expect(getMode()).toBeNull();
  });
});
