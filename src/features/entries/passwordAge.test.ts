import { describe, expect, it } from "vitest";
import type { Entry } from "@/features/vault/types";
import {
  PASSWORD_ROTATION_DAYS,
  formatDays,
  passwordAgeDays,
  passwordChangedAt,
  passwordNeedsRotation,
} from "./passwordAge";

const NOW = new Date("2026-10-20T12:00:00.000Z");

function makeEntry(overrides: Partial<Entry> = {}): Entry {
  return {
    id: "entry-1",
    kind: "login",
    schemaVersion: 1,
    name: "GitHub",
    login: "alex",
    password: "s3cret",
    url: "https://github.com",
    urls: ["https://github.com"],
    comment: "",
    totp: null,
    attachmentIds: [],
    sshKeyIds: [],
    loginHistory: [],
    passwordHistory: [],
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-10-20T12:00:00.000Z",
    ...overrides,
  };
}

describe("passwordAgeDays", () => {
  it("returns null when the entry has no password", () => {
    expect(passwordAgeDays(makeEntry({ password: "" }), NOW)).toBeNull();
    expect(passwordChangedAt(makeEntry({ password: "" }))).toBeNull();
  });

  it("returns 0 for a password saved today", () => {
    expect(passwordAgeDays(makeEntry(), NOW)).toBe(0);
    expect(passwordNeedsRotation(makeEntry(), NOW)).toBe(false);
  });

  it("counts the age from updatedAt when there is no password history", () => {
    const entry = makeEntry({ updatedAt: "2026-04-01T12:00:00.000Z" });
    expect(passwordAgeDays(entry, NOW)).toBe(202);
    expect(passwordNeedsRotation(entry, NOW)).toBe(true);
  });

  it("prefers the newest password history record over updatedAt", () => {
    const entry = makeEntry({
      updatedAt: "2020-01-01T00:00:00.000Z",
      passwordHistory: [
        { value: "older", changedAt: "2026-06-01T12:00:00.000Z" },
        { value: "newest", changedAt: "2026-10-10T12:00:00.000Z" },
      ],
    });
    expect(passwordAgeDays(entry, NOW)).toBe(10);
  });

  it("ignores unusable history timestamps and falls back to updatedAt", () => {
    const entry = makeEntry({
      updatedAt: "2026-10-19T12:00:00.000Z",
      passwordHistory: [{ value: "broken", changedAt: "не дата" }],
    });
    expect(passwordAgeDays(entry, NOW)).toBe(1);
  });

  it("turns red only once the password is 180 days old", () => {
    const fresh = makeEntry({ updatedAt: "2026-04-23T12:00:00.000Z" }); // 180 days
    const almost = makeEntry({ updatedAt: "2026-04-24T12:00:00.000Z" }); // 179 days

    expect(PASSWORD_ROTATION_DAYS).toBe(180);
    expect(passwordAgeDays(almost, NOW)).toBe(179);
    expect(passwordNeedsRotation(almost, NOW)).toBe(false);
    expect(passwordAgeDays(fresh, NOW)).toBe(180);
    expect(passwordNeedsRotation(fresh, NOW)).toBe(true);
  });

  it("never returns a negative age for a clock skewed into the future", () => {
    expect(passwordAgeDays(makeEntry({ updatedAt: "2026-12-01T12:00:00.000Z" }), NOW)).toBe(0);
  });

  it("declines plural forms correctly", () => {
    expect(formatDays(1)).toBe("день");
    expect(formatDays(3)).toBe("дня");
    expect(formatDays(11)).toBe("дней");
    expect(formatDays(21)).toBe("день");
    expect(formatDays(214)).toBe("дней");
  });
});
