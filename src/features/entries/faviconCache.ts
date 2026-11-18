import { invoke } from "@tauri-apps/api/core";
import { createKvStore } from "@/lib/idb";

/** Remains only to clean up the blob left by older versions. */
export const FAVICON_STORAGE_KEY = "pms.favicons.v1";
export const FAVICON_CACHE_TTL_MS = 30 * 24 * 60 * 60 * 1000;
export const FAVICON_MISS_TTL_MS = 60 * 60 * 1000;
export const FAVICON_CACHE_LIMIT = 300;

const DATABASE_NAME = "pms.favicons";
const STORE_NAME = "hosts";

type CacheRecord = {
  dataUrl?: string;
  at: number;
};

const HOST_LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const IMAGE_DATA_URL = /^data:image\/[a-z0-9.+-]+;base64,/i;

/**
 * Session cache: answers repeat questions without touching IndexedDB and keeps
 * working when IndexedDB is unavailable (private mode, old WebView).
 */
const memory = new Map<string, CacheRecord>();
/** Last known write time per host, used to evict the oldest records. */
const timestamps = new Map<string, number>();
const inFlight = new Map<string, Promise<string | null>>();

let store: ReturnType<typeof createKvStore<CacheRecord>> | undefined;

function kv(): ReturnType<typeof createKvStore<CacheRecord>> {
  if (store === undefined) store = createKvStore<CacheRecord>(DATABASE_NAME, STORE_NAME);
  return store;
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

function ttlOf(record: CacheRecord): number {
  return record.dataUrl ? FAVICON_CACHE_TTL_MS : FAVICON_MISS_TTL_MS;
}

function isFresh(record: CacheRecord, now: number): boolean {
  return now - record.at < ttlOf(record);
}

function dropOldestMemory(): void {
  const excess = memory.size - FAVICON_CACHE_LIMIT;
  if (excess <= 0) return;
  const oldest = [...memory.keys()]
    .sort((left, right) => (timestamps.get(left) ?? 0) - (timestamps.get(right) ?? 0))
    .slice(0, excess);
  for (const host of oldest) {
    memory.delete(host);
    timestamps.delete(host);
  }
}

function rememberInMemory(host: string, record: CacheRecord): void {
  memory.delete(host);
  memory.set(host, record);
  timestamps.set(host, record.at);
  dropOldestMemory();
}

/** Keeps the database under `FAVICON_CACHE_LIMIT`, evicting the oldest writes. */
async function trimStore(kvStore: NonNullable<ReturnType<typeof kv>>): Promise<void> {
  const hosts = await kvStore.keys();
  const excess = hosts.length - FAVICON_CACHE_LIMIT;
  if (excess <= 0) return;

  const ages = new Map<string, number>();
  for (const host of hosts) {
    const known = timestamps.get(host);
    if (known !== undefined) {
      ages.set(host, known);
      continue;
    }
    const record = await kvStore.get(host);
    const at = isCacheRecord(record) ? record.at : 0;
    ages.set(host, at);
    timestamps.set(host, at);
  }

  const oldest = hosts
    .sort((left, right) => (ages.get(left) ?? 0) - (ages.get(right) ?? 0))
    .slice(0, excess);
  for (const host of oldest) {
    await kvStore.delete(host);
    timestamps.delete(host);
    memory.delete(host);
  }
}

function forget(host: string): void {
  memory.delete(host);
  timestamps.delete(host);
  const kvStore = kv();
  if (!kvStore) return;
  void kvStore.delete(host).catch(() => undefined);
}

/** Memory lookup only; used before the in-flight dedupe to stay synchronous. */
function fromMemory(host: string, now: number): CacheRecord | null {
  const record = memory.get(host);
  if (!record) return null;
  if (isFresh(record, now)) return record;
  forget(host);
  return null;
}

async function fromStore(host: string, now: number): Promise<CacheRecord | null> {
  const kvStore = kv();
  if (!kvStore) return null;
  try {
    const stored = await kvStore.get(host);
    if (!isCacheRecord(stored)) return null;
    if (!isFresh(stored, now)) {
      await kvStore.delete(host);
      timestamps.delete(host);
      return null;
    }
    rememberInMemory(host, stored);
    return stored;
  } catch {
    // A broken cache must never stop the vault from rendering.
    return null;
  }
}

async function remember(host: string, record: CacheRecord): Promise<void> {
  rememberInMemory(host, record);
  const kvStore = kv();
  if (!kvStore) return;
  try {
    await kvStore.put(host, record);
    await trimStore(kvStore);
  } catch {
    // Quota or blocked database: the in-memory cache still works.
  }
}

async function requestFavicon(host: string): Promise<string | null> {
  const at = Date.now();
  try {
    const dataUrl = await invoke<string>("favicon_fetch", { host });
    if (!IMAGE_DATA_URL.test(dataUrl)) {
      await remember(host, { at });
      return null;
    }
    await remember(host, { dataUrl, at });
    return dataUrl;
  } catch {
    await remember(host, { at });
    return null;
  }
}

async function resolveFavicon(host: string): Promise<string | null> {
  const stored = await fromStore(host, Date.now());
  if (stored) return stored.dataUrl ?? null;
  return requestFavicon(host);
}

export async function loadFavicon(host: string): Promise<string | null> {
  const normalized = normalizeFaviconHost(host);
  if (!normalized) return null;

  const cached = fromMemory(normalized, Date.now());
  if (cached) return cached.dataUrl ?? null;

  const pending = inFlight.get(normalized);
  if (pending) return pending;

  const request = resolveFavicon(normalized).finally(() => {
    if (inFlight.get(normalized) === request) {
      inFlight.delete(normalized);
    }
  });
  inFlight.set(normalized, request);
  return request;
}

export function clearFaviconCache(): void {
  memory.clear();
  timestamps.clear();
  inFlight.clear();

  try {
    localStorage?.removeItem(FAVICON_STORAGE_KEY);
  } catch {
    // Nothing left to clean up.
  }

  const kvStore = kv();
  if (kvStore) void kvStore.clear().catch(() => undefined);
}
