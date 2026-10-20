import type { RecentVault } from "@/lib/storage/types";

const KEY = "pms.recentVaults.v1";
const MAX_RECENT = 20;

function storage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

function isRecentVault(value: unknown): value is RecentVault {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.path === "string" &&
    typeof item.name === "string" &&
    typeof item.vaultId === "string" &&
    typeof item.lastOpenedAt === "string"
  );
}

function save(items: RecentVault[]) {
  storage()?.setItem(KEY, JSON.stringify(items.slice(0, MAX_RECENT)));
}

export function listRecent(): RecentVault[] {
  const raw = storage()?.getItem(KEY);
  if (!raw) return [];

  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(isRecentVault)
      .sort((a, b) => b.lastOpenedAt.localeCompare(a.lastOpenedAt))
      .slice(0, MAX_RECENT);
  } catch {
    return [];
  }
}

export function addRecent(entry: RecentVault): RecentVault[] {
  const next = [entry, ...listRecent().filter((item) => item.path !== entry.path)].slice(
    0,
    MAX_RECENT,
  );
  save(next);
  return next;
}

export function removeRecent(path: string): RecentVault[] {
  const next = listRecent().filter((item) => item.path !== path);
  save(next);
  return next;
}

export function lastRecent(): RecentVault | null {
  return listRecent()[0] ?? null;
}
