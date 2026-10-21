import { describe, expect, it } from "vitest";
import type { Entry } from "@/features/vault/types";
import {
  createEntry,
  emptyEntryDraft,
  entryToDraft,
  entryUrls,
  findDuplicatePasswords,
  normalizeEntryDraft,
  removeEntry,
  searchEntries,
  sortEntries,
  updateEntry,
  type EntryDraft,
} from "./entries";

const now = "2026-10-20T10:00:00.000Z";

function draft(overrides: Partial<EntryDraft> = {}): EntryDraft {
  return {
    ...emptyEntryDraft,
    name: "GitHub",
    login: "alex",
    password: "s3cret",
    ...overrides,
  };
}

function entry(overrides: Partial<Entry> = {}): Entry {
  return {
    ...createEntry(draft(), now),
    ...overrides,
  };
}

describe("entries", () => {
  it("creates an entry with a hidden id and timestamps", () => {
    const created = createEntry(draft(), now);

    expect(created.id).toMatch(/[0-9a-f-]{36}/);
    expect(created.name).toBe("GitHub");
    expect(created.kind).toBe("login");
    expect(created.createdAt).toBe(now);
    expect(created.updatedAt).toBe(now);
    expect(created.loginHistory).toEqual([]);
    expect(created.passwordHistory).toEqual([]);
  });

  it("normalizes multiple urls and mirrors the first one", () => {
    const created = createEntry(
      draft({
        urls: [" https://a.example ", "https://b.example", ""],
      }),
      now,
    );

    expect(created.urls).toEqual(["https://a.example", "https://b.example"]);
    expect(created.url).toBe("https://a.example");
    expect(entryUrls(created)).toEqual(created.urls);
    expect(entryUrls({ url: "https://legacy.example", urls: [] })).toEqual([
      "https://legacy.example",
    ]);
  });

  it("converts an entry to and from a draft", () => {
    const created = entry();
    expect(entryToDraft(created)).toEqual({
      name: "GitHub",
      login: "alex",
      password: "s3cret",
      urls: [],
      comment: "",
      attachmentIds: [],
      sshKeyIds: [],
      totp: null,
      totpRaw: "",
    });
  });

  it("preserves a non-default TOTP configuration when only other fields change", () => {
    const totp = {
      secret: "JBSWY3DPEHPK3PXP",
      algorithm: "SHA256" as const,
      digits: 8 as const,
      period: 60 as const,
      issuer: "GitHub",
      account: "alex",
    };
    const original = entry({ totp });
    const result = updateEntry(
      original,
      { ...entryToDraft(original), name: "GitHub renamed" },
      now,
    );

    expect(result.entry.totp).toEqual(totp);
    expect(result.entry.name).toBe("GitHub renamed");
  });

  it("normalizes drafts created before attachments and TOTP existed", () => {
    const oldDraft = {
      name: "Старый черновик",
      login: "alex",
      password: "s3cret",
      url: "https://legacy.example",
      comment: "",
    } as Parameters<typeof normalizeEntryDraft>[0];

    expect(normalizeEntryDraft(oldDraft)).toEqual({
      ...oldDraft,
      urls: ["https://legacy.example"],
      attachmentIds: [],
      sshKeyIds: [],
      totp: null,
      totpRaw: "",
    });
  });

  it("moves old login and password into history on change", () => {
    const original = entry();
    const result = updateEntry(
      original,
      draft({ login: "alex2", password: "new-secret" }),
      "2026-10-21T10:00:00.000Z",
    );

    expect(result.blockedReason).toBeUndefined();
    expect(result.entry.loginHistory).toEqual([
      { value: "alex", changedAt: "2026-10-21T10:00:00.000Z" },
    ]);
    expect(result.entry.passwordHistory).toEqual([
      { value: "s3cret", changedAt: "2026-10-21T10:00:00.000Z" },
    ]);
    expect(result.entry.updatedAt).toBe("2026-10-21T10:00:00.000Z");
  });

  it("does not record empty old values", () => {
    const original = entry({ login: "", password: "" });
    const result = updateEntry(original, draft({ login: "new", password: "new" }), now);

    expect(result.entry.loginHistory).toEqual([]);
    expect(result.entry.passwordHistory).toEqual([]);
  });

  it("blocks reuse of a historical password", () => {
    const original = entry({
      passwordHistory: [{ value: "old-secret", changedAt: now }],
    });
    const result = updateEntry(original, draft({ password: "old-secret" }), now);

    expect(result.blockedReason).toBe("old-password");
    expect(result.entry).toEqual(original);
  });

  it("allows keeping the current password", () => {
    const original = entry();
    const result = updateEntry(original, draft({ password: original.password }), now);

    expect(result.blockedReason).toBeUndefined();
    expect(result.entry.passwordHistory).toEqual([]);
  });

  it("finds duplicate passwords only in other entries", () => {
    const first = entry({ id: "a", name: "A", password: "same" });
    const second = entry({ id: "b", name: "B", password: "same" });
    const third = entry({ id: "c", name: "C", password: "unique" });

    expect(
      findDuplicatePasswords([first, second, third], "same", "a").map((item) => item.name),
    ).toEqual(["B"]);
    expect(findDuplicatePasswords([first, second], "", "a")).toEqual([]);
  });

  it("searches name, login, urls and comment case-insensitively", () => {
    const first = entry({
      id: "a",
      name: "GitHub",
      login: "alex",
      url: "https://github.com",
      urls: ["https://github.com", "https://gist.github.com"],
    });
    const second = entry({
      id: "b",
      name: "GitLab",
      login: "work",
      url: "https://gitlab.com",
      urls: ["https://gitlab.com"],
      comment: "рабочий",
    });

    expect(searchEntries([first, second], "git").map((item) => item.name)).toEqual([
      "GitHub",
      "GitLab",
    ]);
    expect(searchEntries([first, second], "РАБОЧ").map((item) => item.name)).toEqual([
      "GitLab",
    ]);
    expect(searchEntries([first, second], "gist.github").map((item) => item.name)).toEqual([
      "GitHub",
    ]);
    expect(searchEntries([first, second], "gitlab.com").map((item) => item.name)).toEqual([
      "GitLab",
    ]);
  });

  it("sorts by name and by last update", () => {
    const first = entry({ id: "a", name: "Бета", updatedAt: "2026-10-20T10:00:00.000Z" });
    const second = entry({ id: "b", name: "Альфа", updatedAt: "2026-10-21T10:00:00.000Z" });

    expect(sortEntries([first, second], "name").map((item) => item.name)).toEqual([
      "Альфа",
      "Бета",
    ]);
    expect(sortEntries([first, second], "updated").map((item) => item.name)).toEqual([
      "Альфа",
      "Бета",
    ]);
  });

  it("caps history at 50 values", () => {
    let current = entry();
    for (let index = 0; index < 60; index += 1) {
      const result = updateEntry(
        current,
        draft({ password: `password-${index}` }),
        `2026-10-20T10:${String(index).padStart(2, "0")}:00.000Z`,
      );
      current = result.entry;
    }

    expect(current.passwordHistory).toHaveLength(50);
    expect(current.passwordHistory[0]?.value).toBe("password-58");
  });

  it("removes an entry by id", () => {
    const first = entry({ id: "a" });
    const second = entry({ id: "b" });

    expect(removeEntry([first, second], "a")).toEqual([second]);
  });
});
