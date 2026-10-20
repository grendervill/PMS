import type { Entry, HistoryItem } from "@/features/vault/types";

export type EntryDraft = {
  name: string;
  login: string;
  password: string;
  url: string;
  comment: string;
};

export type UpdateEntryResult = {
  entry: Entry;
  blockedReason?: "old-password";
};

export type SortMode = "name" | "updated";

const HISTORY_LIMIT = 50;

export const emptyEntryDraft: EntryDraft = {
  name: "",
  login: "",
  password: "",
  url: "",
  comment: "",
};

export function entryToDraft(entry: Entry): EntryDraft {
  return {
    name: entry.name,
    login: entry.login,
    password: entry.password,
    url: entry.url,
    comment: entry.comment,
  };
}

export function createEntry(
  draft: EntryDraft,
  now = new Date().toISOString(),
): Entry {
  return {
    id: crypto.randomUUID(),
    kind: "login",
    schemaVersion: 1,
    name: draft.name.trim(),
    login: draft.login,
    password: draft.password,
    url: draft.url.trim(),
    comment: draft.comment,
    totp: null,
    attachmentIds: [],
    sshKeyIds: [],
    loginHistory: [],
    passwordHistory: [],
    createdAt: now,
    updatedAt: now,
  };
}

export function updateEntry(
  entry: Entry,
  draft: EntryDraft,
  now = new Date().toISOString(),
): UpdateEntryResult {
  const passwordChanged = draft.password !== entry.password;
  const reusesOldPassword =
    passwordChanged && entry.passwordHistory.some((item) => item.value === draft.password);

  if (reusesOldPassword) {
    return { entry, blockedReason: "old-password" };
  }

  let loginHistory = entry.loginHistory;
  if (draft.login !== entry.login && entry.login) {
    loginHistory = pushHistory(loginHistory, entry.login, now);
  }

  let passwordHistory = entry.passwordHistory;
  if (passwordChanged && entry.password) {
    passwordHistory = pushHistory(passwordHistory, entry.password, now);
  }

  return {
    entry: {
      ...entry,
      name: draft.name.trim(),
      login: draft.login,
      password: draft.password,
      url: draft.url.trim(),
      comment: draft.comment,
      loginHistory,
      passwordHistory,
      updatedAt: now,
    },
  };
}

export function removeEntry(entries: Entry[], id: string): Entry[] {
  return entries.filter((entry) => entry.id !== id);
}

export function monogram(name: string): string {
  const first = name.trim().charAt(0);
  return first ? first.toLocaleUpperCase("ru") : "•";
}

export function safeHttpUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  try {
    const url = new URL(trimmed.includes("://") ? trimmed : `https://${trimmed}`);
    return url.protocol === "http:" || url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

export function hostFromUrl(value: string): string {
  const url = safeHttpUrl(value);
  if (!url) return value;
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return value;
  }
}

export function findDuplicatePasswords(
  entries: Entry[],
  password: string,
  excludeId?: string,
): Entry[] {
  if (!password) return [];
  return entries.filter(
    (entry) =>
      entry.id !== excludeId && entry.password.length > 0 && entry.password === password,
  );
}

export function searchEntries(entries: Entry[], query: string): Entry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;

  return entries.filter((entry) =>
    [entry.name, entry.login, entry.url, entry.comment]
      .join("\n")
      .toLowerCase()
      .includes(needle),
  );
}

export function sortEntries(entries: Entry[], mode: SortMode): Entry[] {
  return [...entries].sort((a, b) => {
    if (mode === "name") {
      return a.name.localeCompare(b.name, "ru", { sensitivity: "base" });
    }
    return b.updatedAt.localeCompare(a.updatedAt);
  });
}

function pushHistory(
  history: HistoryItem[],
  value: string,
  now: string,
): HistoryItem[] {
  const next = history.filter((item) => item.value !== value);
  return [{ value, changedAt: now }, ...next].slice(0, HISTORY_LIMIT);
}
