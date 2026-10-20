import { describe, expect, it } from "vitest";
import { errorCode, errorText } from "./errors";

describe("vault error messages", () => {
  it("maps known codes to Russian messages", () => {
    expect(errorText({ code: "invalid_credentials", message: "bad" })).toContain(
      "Не удалось разблокировать",
    );
    expect(errorText({ code: "bad_magic", message: "bad file magic" })).toContain(
      "повреждён",
    );
    expect(errorText({ code: "storage", message: "io" })).toContain("папке");
  });

  it("never exposes raw English internals", () => {
    expect(errorText({ code: "some_internal", message: "serde: missing field" })).toBe(
      "Не удалось выполнить операцию.",
    );
  });

  it("passes through a Russian message when the code is unknown", () => {
    expect(errorText({ code: "custom", message: "Понятная ошибка" })).toBe(
      "Понятная ошибка",
    );
  });

  it("extracts codes", () => {
    expect(errorCode({ code: "storage" })).toBe("storage");
    expect(errorCode(new Error("x"))).toBeNull();
  });
});
