import { invoke } from "@tauri-apps/api/core";

export const FAVICON_STORAGE_KEY = "pms.favicons.v1";
export const FAVICON_CACHE_TTL_MS = 30 * 24 * 60 * 60 * 1000;
export const FAVICON_MISS_TTL_MS = 60 * 60 * 1000;
export const FAVICON_CACHE_LIMIT = 300;

type CacheRecord = {
  dataUrl?: string;
  at: number;
};

type CacheState = {
  version: 1;
  hosts: Record<string, CacheRecord>;
};

const HOST_LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const IMAGE_DATA_URL = /^data:image\/[a-z0-9.+-]+;base64,/i;

let state: CacheState | null = null;
const inFlight = new Map<string, Promise<string | null>>();

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function normalizeFaviconHost(value: string): string | null {
  const trimmed = value.trim().toLowerCase().replace(/\.$/, "");
  if (!trimmed) return null;
  const host = trimmed.startsWith("www.") ? trimmed.slice(4) : trimmed;
  if (!host || host.length > 253) return null;

  const labels = host.split(".");
  if (labels.length < 2 || !labels.every((label) => HOST_LABEL.test(label))) {
    return null;
  }
  return host;
}

function isCacheRecord(value: unknown): value is CacheRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as { dataUrl?: unknown; at?: unknown };
  if (typeof record.at !== "number" || !Number.isFinite(record.at)) return false;
  if (record.dataUrl === undefined) return true;
  return typeof record.dataUrl === "string" && IMAGE_DATA_URL.test(record.dataUrl);
}

function readState(): CacheState {
  if (state) return state;

  const empty: CacheState = { version: 1, hosts: {} };
  const raw = storage()?.getItem(FAVICON_STORAGE_KEY);
  if (!raw) {
    state = empty;
    return state;
  }

  try {
    const parsed = JSON.parse(raw) as { hosts?: unknown };
    if (parsed && typeof parsed === "object" && parsed.hosts && typeof parsed.hosts === "object") {
      for (const [host, record] of Object.entries(parsed.hosts)) {
        if (normalizeFaviconHost(host) === host && isCacheRecord(record)) {
          empty.hosts[host] = record;
        }
      }
    }
  } catch {
    // A broken cache must never stop the vault from rendering.
  }

  state = empty;
  return state;
}

function persist(): void {
  if (!state) return;
  try {
    storage()?.setItem(FAVICON_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // The in-memory cache still works when storage is full or unavailable.
  }
}

function freshRecord(host: string): CacheRecord | null {
  const current = readState();
  const record = current.hosts[host];
  if (!record) return null;

  const ttl = record.dataUrl ? FAVICON_CACHE_TTL_MS : FAVICON_MISS_TTL_MS;
  if (Date.now() - record.at >= ttl) {
    delete current.hosts[host];
    persist();
    return null;
  }
  return record;
}

function remember(host: string, record: CacheRecord): void {
  const current = readState();
  delete current.hosts[host];
  current.hosts[host] = record;

  const keys = Object.keys(current.hosts);
  const overflow = keys.length - FAVICON_CACHE_LIMIT;
  if (overflow > 0) {
    for (const key of keys.slice(0, overflow)) {
      delete current.hosts[key];
    }
  }
  persist();
}

async function requestFavicon(host: string): Promise<string | null> {
  try {
    const dataUrl = await invoke<string>("favicon_fetch", { host });
    if (!IMAGE_DATA_URL.test(dataUrl)) {
      remember(host, { at: Date.now() });
      return null;
    }
    remember(host, { dataUrl, at: Date.now() });
    return dataUrl;
  } catch {
    remember(host, { at: Date.now() });
    return null;
  }
}

export async function loadFavicon(host: string): Promise<string | null> {
  const normalized = normalizeFaviconHost(host);
  if (!normalized) return null;

  const cached = freshRecord(normalized);
  if (cached) return cached.dataUrl ?? null;

  const pending = inFlight.get(normalized);
  if (pending) return pending;

  const request = requestFavicon(normalized).finally(() => {
    if (inFlight.get(normalized) === request) {
      inFlight.delete(normalized);
    }
  });
  inFlight.set(normalized, request);
  return request;
}

export function clearFaviconCache(): void {
  state = { version: 1, hosts: {} };
  inFlight.clear();
  try {
    storage()?.removeItem(FAVICON_STORAGE_KEY);
  } catch {
    // Nothing else to clear.
  }
}
