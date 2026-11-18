import { describe, expect, it } from "vitest";
import {
  MAX_FORBIDDEN_CHARS,
  emptyPasswordPolicy,
  isPasswordPolicyEmpty,
  normalizeForbiddenChars,
  normalizePasswordPolicy,
  passwordPolicyError,
  passwordPolicySummary,
} from "./passwordPolicy";

const policy = {
  minLength: 12,
  maxLength: 20,
  requireLowercase: false,
  requireUppercase: true,
  requireDigits: true,
  requireSymbols: false,
  forbiddenChars: "<>",
};

describe("normalizePasswordPolicy", () => {
  it("turns an all-default policy into nothing at all", () => {
    expect(normalizePasswordPolicy(emptyPasswordPolicy())).toBeUndefined();
    expect(normalizePasswordPolicy(undefined)).toBeUndefined();
    expect(normalizePasswordPolicy(null)).toBeUndefined();
    expect(normalizePasswordPolicy({ minLength: 0, forbiddenChars: "" })).toBeUndefined();
  });

  it("keeps a real policy and drops unknown numbers", () => {
    expect(normalizePasswordPolicy(policy)).toEqual(policy);
    expect(normalizePasswordPolicy({ ...policy, minLength: Number.NaN })).toEqual({
      ...policy,
      minLength: 0,
    });
  });

  it("never lets the minimum exceed the maximum or the generator ceiling", () => {
    expect(normalizePasswordPolicy({ ...policy, minLength: 30, maxLength: 20 })).toEqual({
      ...policy,
      minLength: 20,
      maxLength: 20,
    });
    expect(normalizePasswordPolicy({ ...policy, minLength: 400, maxLength: 0 })).toEqual({
      ...policy,
      minLength: 128,
      maxLength: 0,
    });
  });

  it("deduplicates and caps the forbidden characters", () => {
    expect(normalizeForbiddenChars("<<>>.. \"'")).toBe("<>.\"'");
    expect(normalizeForbiddenChars("")).toBe("");
    expect(normalizeForbiddenChars(undefined)).toBe("");
    const many = Array.from({ length: 200 }, (_, index) =>
      String.fromCharCode(33 + (index % 90)),
    ).join("");
    expect(normalizeForbiddenChars(many)).toHaveLength(MAX_FORBIDDEN_CHARS);
  });
});

describe("passwordPolicySummary", () => {
  it("describes the range, the required sets and the banned characters", () => {
    expect(passwordPolicySummary(policy)).toBe(
      "длина 12–20 · обязательны: заглавные буквы, цифры · запрещены <>",
    );
  });

  it("works with a single bound and no sets", () => {
    expect(
      passwordPolicySummary({
        ...emptyPasswordPolicy(),
        minLength: 14,
      }),
    ).toBe("не короче 14");
    expect(
      passwordPolicySummary({
        ...emptyPasswordPolicy(),
        maxLength: 16,
      }),
    ).toBe("не длиннее 16");
  });

  it("is empty for an empty policy", () => {
    expect(passwordPolicySummary(emptyPasswordPolicy())).toBe("");
    expect(isPasswordPolicyEmpty(emptyPasswordPolicy())).toBe(true);
    expect(isPasswordPolicyEmpty(policy)).toBe(false);
  });
});

describe("passwordPolicyError", () => {
  it("rejects a minimum above the maximum", () => {
    expect(passwordPolicyError({ ...policy, minLength: 30, maxLength: 20 })).toBe(
      "Минимальная длина больше максимальной",
    );
  });

  it("rejects a minimum the generator cannot reach", () => {
    expect(passwordPolicyError({ ...policy, minLength: 200 })).toMatch(/не может быть больше/);
  });

  it("accepts a usable policy", () => {
    expect(passwordPolicyError(policy)).toBeNull();
    expect(passwordPolicyError({ ...policy, minLength: 20, maxLength: 20 })).toBeNull();
  });
});
