import type { VaultStorage } from "@/lib/storage";

/**
 * Состояние синхронизации профиля. Лежит в кэше рядом с сейфом: если кэш
 * вычистить, базовая версия исчезнет вместе с ним, а не останется врать.
 */
export type SyncState = {
  /** Версия сервера, с которой сверяется локальный сейф. */
  baseVersion: number;
  /** Локальные изменения, которые ещё не уехали. */
  dirty: boolean;
  lastSyncedAt: string | null;
  lastError: string | null;
};

export const SYNC_FILE = "sync.json";

export function emptyState(): SyncState {
  return { baseVersion: 0, dirty: false, lastSyncedAt: null, lastError: null };
}

function isState(value: unknown): value is SyncState {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.baseVersion === "number" &&
    Number.isFinite(item.baseVersion) &&
    typeof item.dirty === "boolean" &&
    (item.lastSyncedAt === null || typeof item.lastSyncedAt === "string") &&
    (item.lastError === null || typeof item.lastError === "string")
  );
}

export async function readSyncState(
  storage: VaultStorage,
  cacheDir: string,
): Promise<SyncState> {
  try {
    const bytes = await storage.readFile(`${cacheDir}/${SYNC_FILE}`);
    const parsed: unknown = JSON.parse(new TextDecoder().decode(bytes));
    if (isState(parsed)) return parsed;
  } catch {
    // Нет файла или он испорчен — начинаем с чистой базовой версии.
  }
  return emptyState();
}

export async function writeSyncState(
  storage: VaultStorage,
  cacheDir: string,
  state: SyncState,
): Promise<void> {
  const bytes = new TextEncoder().encode(JSON.stringify(state));
  await storage.saveFile(`${cacheDir}/${SYNC_FILE}`, bytes);
}
