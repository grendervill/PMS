/**
 * Last generated values, kept in `sessionStorage` on purpose: the history is a
 * convenience for the current window, never something that survives a restart.
 * Every access is defensive — blocked storage (private mode, disabled cookies,
 * exotic webviews) must not break the generator.
 */

export type GeneratorHistoryMode = "password" | "login" | "passphrase";

export type GeneratorHistoryEntry = {
  value: string;
  mode: GeneratorHistoryMode;
  at: number;
};

export const GENERATOR_HISTORY_KEY = "pms.generator.history.v1";
export const GENERATOR_HISTORY_LIMIT = 20;
/** Characters kept in the compact list before the ellipsis. */
const HISTORY_PREVIEW_CHARS = 12;

const MODES: GeneratorHistoryMode[] = ["password", "login", "passphrase"];

function historyStorage(): Storage | null {
  try {
    return globalThis.sessionStorage ?? null;
  } catch {
    // Reading the property itself throws when storage is blocked.
    return null;
  }
}

function isMode(value: unknown): value is GeneratorHistoryMode {
  return typeof value === "string" && MODES.some((mode) => mode === value);
}

function sanitize(raw: unknown): GeneratorHistoryEntry[] {
  if (!Array.isArray(raw)) return [];
  const entries: GeneratorHistoryEntry[] = [];
  for (const item of raw) {
    if (entries.length >= GENERATOR_HISTORY_LIMIT) break;
    if (!item || typeof item !== "object") continue;
    const candidate = item as Partial<GeneratorHistoryEntry>;
    if (typeof candidate.value !== "string" || candidate.value.length === 0) continue;
    if (!isMode(candidate.mode)) continue;
    entries.push({
      value: candidate.value,
      mode: candidate.mode,
      at: typeof candidate.at === "number" && Number.isFinite(candidate.at) ? candidate.at : 0,
    });
  }
  return entries;
}

/** Never throws: a missing, blocked or corrupted store reads as an empty list. */
export function readGeneratorHistory(): GeneratorHistoryEntry[] {
  const storage = historyStorage();
  if (!storage) return [];
  try {
    const raw = storage.getItem(GENERATOR_HISTORY_KEY);
    if (!raw) return [];
    return sanitize(JSON.parse(raw) as unknown);
  } catch {
    return [];
  }
}

function writeGeneratorHistory(entries: GeneratorHistoryEntry[]): void {
  const storage = historyStorage();
  if (!storage) return;
  try {
    storage.setItem(GENERATOR_HISTORY_KEY, JSON.stringify(entries));
  } catch {
    // Quota or blocked storage: the in-memory list still works for this mount.
  }
}

/**
 * Adds a value to the front of the list and returns the new list. Re-generating
 * an identical value moves it up instead of duplicating the row.
 */
export function pushGeneratorHistory(entry: {
  value: string;
  mode: GeneratorHistoryMode;
}): GeneratorHistoryEntry[] {
  if (!entry.value) return readGeneratorHistory();

  const rest = readGeneratorHistory().filter(
    (item) => !(item.value === entry.value && item.mode === entry.mode),
  );
  const next: GeneratorHistoryEntry[] = [
    { value: entry.value, mode: entry.mode, at: Date.now() },
    ...rest,
  ].slice(0, GENERATOR_HISTORY_LIMIT);

  writeGeneratorHistory(next);
  return next;
}

/**
 * Exported for the vault store: call it from `lock()`/`reset()` so a locked
 * vault leaves no generated secrets behind. The panel itself never calls it.
 */
export function clearGeneratorHistory(): void {
  const storage = historyStorage();
  if (!storage) return;
  try {
    storage.removeItem(GENERATOR_HISTORY_KEY);
  } catch {
    // Nothing to do: the list is gone with the window anyway.
  }
}

/** Compact one-line preview; the full value stays in the row's `title`. */
export function summarizeHistoryValue(value: string, limit = HISTORY_PREVIEW_CHARS): string {
  const characters = Array.from(value);
  if (characters.length < limit) return value;
  return `${characters.slice(0, limit - 1).join("")}…`;
}
