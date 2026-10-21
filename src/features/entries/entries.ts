import type { Entry, HistoryItem, TotpConfig } from "@/features/vault/types";

export type EntryDraft = {
  name: string;
  login: string;
  password: string;
  urls: string[];
  comment: string;
  attachmentIds: string[];
  sshKeyIds: string[];
  totp: TotpConfig | null;
  totpRaw: string;
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
  urls: [],
  comment: "",
  attachmentIds: [],
  sshKeyIds: [],
  totp: null,
  totpRaw: "",
};

export function normalizeUrlList(values: string[]): string[] {
  return values.map((value) => value.trim()).filter(Boolean);
}

export function entryUrls(entry: Pick<Entry, "url" | "urls">): string[] {
  const urls = entry.urls ?? [];
  return urls.length > 0 ? urls : entry.url ? [entry.url] : [];
}

type LegacyEntryDraft = Partial<EntryDraft> & { url?: string };

export function normalizeEntryDraft(value?: LegacyEntryDraft | null): EntryDraft {
  const normalized = { ...emptyEntryDraft, ...(value ?? {}) };
  return {
    ...normalized,
    urls: value?.urls ?? (value?.url ? [value.url] : []),
    attachmentIds: value?.attachmentIds ?? [],
    sshKeyIds: value?.sshKeyIds ?? [],
    totp: value?.totp ?? null,
    totpRaw: value?.totpRaw ?? value?.totp?.secret ?? "",
  };
}

export function entryToDraft(entry: Entry): EntryDraft {
  return {
    name: entry.name,
    login: entry.login,
    password: entry.password,
    urls: entryUrls(entry),
    comment: entry.comment,
    attachmentIds: [...entry.attachmentIds],
    sshKeyIds: [...entry.sshKeyIds],
    totp: entry.totp ?? null,
    totpRaw: entry.totp?.secret ?? "",
  };
}

export function createEntry(
  draft: EntryDraft,
  now = new Date().toISOString(),
): Entry {
  const urls = normalizeUrlList(draft.urls);
  return {
    id: crypto.randomUUID(),
    kind: "login",
    schemaVersion: 1,
    name: draft.name.trim(),
    login: draft.login,
    password: draft.password,
    url: urls[0] ?? "",
    urls,
    comment: draft.comment,
    totp: draft.totp,
    attachmentIds: [...draft.attachmentIds],
    sshKeyIds: [...draft.sshKeyIds],
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

  const urls = normalizeUrlList(draft.urls);
  return {
    entry: {
      ...entry,
      name: draft.name.trim(),
      login: draft.login,
      password: draft.password,
      url: urls[0] ?? "",
      urls,
      comment: draft.comment,
      totp: draft.totp,
      attachmentIds: [...draft.attachmentIds],
      sshKeyIds: [...draft.sshKeyIds],
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

export function groupDuplicateEntries(entries: Entry[]): Entry[][] {
  const groups = new Map<string, Entry[]>();
  for (const entry of entries) {
    if (!entry.password) continue;
    const group = groups.get(entry.password) ?? [];
    group.push(entry);
    groups.set(entry.password, group);
  }
  return [...groups.values()].filter((group) => group.length > 1);
}

export function searchEntries(entries: Entry[], query: string): Entry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;

  return entries.filter((entry) =>
    [entry.name, entry.login, entryUrls(entry).join(" "), entry.comment]
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
