/**
 * Где живёт сейф: в папке на диске или на сервере синхронизации.
 *
 * Выбор запоминается, чтобы при следующем запуске не спрашивать заново.
 * Секретов здесь нет — только режим.
 */

export type StorageMode = "local" | "remote";

const MODE_KEY = "pms.storageMode.v1";

function storage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

export function getMode(): StorageMode | null {
  const raw = storage()?.getItem(MODE_KEY);
  return raw === "local" || raw === "remote" ? raw : null;
}

export function setMode(mode: StorageMode): void {
  storage()?.setItem(MODE_KEY, mode);
}

export function clearMode(): void {
  storage()?.removeItem(MODE_KEY);
}
