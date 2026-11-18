import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const ICON = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
const DATABASE = "pms.favicons";
const STORE = "hosts";

type FakeRequest = {
  result: unknown;
  error: DOMException | null;
  onsuccess: ((event: Event) => void) | null;
  onerror: ((event: Event) => void) | null;
  onupgradeneeded: ((event: Event) => void) | null;
};

type FakeDatabase = { version: number; stores: Map<string, Map<string, unknown>> };

/**
 * Just enough of IndexedDB for the wrapper: open/upgradeneeded, one object
 * store, and get/put/delete/clear/getAllKeys inside a transaction. Requests
 * settle in microtasks so the cache code is exercised exactly as in a browser.
 */
function createFakeIndexedDB() {
  const databases = new Map<string, FakeDatabase>();

  const createTransaction = (data: Map<string, unknown>) => {
    const transaction = {
      error: null as DOMException | null,
      oncomplete: null as ((event: Event) => void) | null,
      onerror: null as ((event: Event) => void) | null,
      onabort: null as ((event: Event) => void) | null,
      objectStore: () => createObjectStore(data, transaction),
    };
    return transaction;
  };

  const createObjectStore = (
    data: Map<string, unknown>,
    transaction: ReturnType<typeof createTransaction>,
  ) => {
    const run = (action: () => unknown): FakeRequest => {
      const request: FakeRequest = {
        result: undefined,
        error: null,
        onsuccess: null,
        onerror: null,
        onupgradeneeded: null,
      };
      queueMicrotask(() => {
        try {
          request.result = action();
          transaction.oncomplete?.({} as Event);
        } catch (error) {
          request.error = error as DOMException;
          queueMicrotask(() => {
            transaction.onerror?.({} as Event);
            transaction.onabort?.({} as Event);
          });
        }
      });
      return request;
    };

    return {
      get: (key: string) => run(() => data.get(key)) as unknown as IDBRequest,
      put: (value: unknown, key: string) =>
        run(() => {
          data.set(key, value);
          return key;
        }) as unknown as IDBRequest,
      delete: (key: string) =>
        run(() => {
          data.delete(key);
          return undefined;
        }) as unknown as IDBRequest,
      clear: () =>
        run(() => {
          data.clear();
          return undefined;
        }) as unknown as IDBRequest,
      getAllKeys: () => run(() => [...data.keys()]) as unknown as IDBRequest,
    };
  };

  const factory = {
    open(name: string, version = 1) {
      const request: FakeRequest = {
        result: undefined,
        error: null,
        onsuccess: null,
        onerror: null,
        onupgradeneeded: null,
      };

      queueMicrotask(() => {
        const existing = databases.get(name);
        const needsUpgrade = !existing || existing.version < version;
        const record: FakeDatabase = existing ?? { version, stores: new Map() };
        databases.set(name, record);

        const database = {
          objectStoreNames: { contains: (store: string) => record.stores.has(store) },
          createObjectStore: (store: string) => {
            if (!record.stores.has(store)) record.stores.set(store, new Map());
            return {};
          },
          transaction: (store: string) => {
            const data = record.stores.get(store);
            if (!data) throw new Error(`missing store: ${store}`);
            return createTransaction(data);
          },
          close: () => undefined,
        };

        request.result = database;
        if (needsUpgrade) request.onupgradeneeded?.({} as Event);
        request.onsuccess?.({} as Event);
      });

      return request as unknown as IDBOpenDBRequest;
    },
  };

  return {
    factory: factory as unknown as IDBFactory,
    records: (name = DATABASE, store = STORE) => databases.get(name)?.stores.get(store),
  };
}

async function flush(): Promise<void> {
  for (let step = 0; step < 25; step += 1) await Promise.resolve();
}

async function loadCache() {
  vi.resetModules();
  return await import("./faviconCache");
}

let fake: ReturnType<typeof createFakeIndexedDB>;

beforeEach(() => {
  localStorage.clear();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(ICON);
  fake = createFakeIndexedDB();
  vi.stubGlobal("indexedDB", fake.factory);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("favicon cache", () => {
  it("reuses a cached icon for every card with the same host", async () => {
    const cache = await loadCache();

    expect(await cache.loadFavicon("github.com")).toBe(ICON);
    expect(await cache.loadFavicon("github.com")).toBe(ICON);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("favicon_fetch", { host: "github.com" });
    expect([...(fake.records()?.keys() ?? [])]).toEqual(["github.com"]);
  });

  it("writes a single record per host instead of the whole map", async () => {
    const cache = await loadCache();

    await cache.loadFavicon("github.com");
    await cache.loadFavicon("gitlab.com");
    await cache.loadFavicon("github.com");

    const records = fake.records();
    expect(records?.size).toBe(2);
    expect(records?.get("github.com")).toMatchObject({ dataUrl: ICON });
  });

  it("shares one request between cards mounted at the same time", async () => {
    const cache = await loadCache();
    let resolveRequest: (value: string) => void = () => undefined;
    invokeMock.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveRequest = resolve;
      }),
    );

    const first = cache.loadFavicon("github.com");
    const second = cache.loadFavicon("github.com");
    await flush();

    expect(invokeMock).toHaveBeenCalledTimes(1);
    resolveRequest(ICON);
    await expect(first).resolves.toBe(ICON);
    await expect(second).resolves.toBe(ICON);
  });

  it("normalizes case, www and the trailing dot before caching", async () => {
    const cache = await loadCache();

    expect(await cache.loadFavicon("WWW.GitHub.com.")).toBe(ICON);
    expect(await cache.loadFavicon("github.com")).toBe(ICON);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("favicon_fetch", { host: "github.com" });
  });

  it("ignores values that are not hostnames", async () => {
    const cache = await loadCache();

    await expect(cache.loadFavicon("https://github.com/path")).resolves.toBeNull();
    await expect(cache.loadFavicon("")).resolves.toBeNull();
    await expect(cache.loadFavicon("localhost")).resolves.toBeNull();

    expect(invokeMock).not.toHaveBeenCalled();
    expect(cache.normalizeFaviconHost("sub.example.co.uk")).toBe("sub.example.co.uk");
  });

  it("downloads an icon again after the cache expires", async () => {
    const cache = await loadCache();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-23T10:00:00.000Z"));
    await cache.loadFavicon("github.com");

    vi.setSystemTime(new Date(Date.now() + cache.FAVICON_CACHE_TTL_MS + 1000));
    await cache.loadFavicon("github.com");

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("remembers a failed lookup briefly and retries after an hour", async () => {
    const cache = await loadCache();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-23T10:00:00.000Z"));
    invokeMock.mockRejectedValue(new Error("network"));

    await expect(cache.loadFavicon("missing.example")).resolves.toBeNull();
    await expect(cache.loadFavicon("missing.example")).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(new Date(Date.now() + cache.FAVICON_MISS_TTL_MS + 1000));
    await expect(cache.loadFavicon("missing.example")).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("keeps icons between app launches", async () => {
    const first = await loadCache();
    await first.loadFavicon("github.com");

    invokeMock.mockRejectedValue(new Error("must not fetch"));
    const second = await loadCache();

    await expect(second.loadFavicon("github.com")).resolves.toBe(ICON);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("limits the number of stored hosts, evicting the oldest writes", async () => {
    const cache = await loadCache();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-23T10:00:00.000Z"));

    const total = cache.FAVICON_CACHE_LIMIT + 5;
    for (let index = 0; index < total; index += 1) {
      await cache.loadFavicon(`site-${index}.example`);
      vi.setSystemTime(new Date(Date.now() + 1000));
    }

    const records = fake.records();
    expect(records?.size).toBe(cache.FAVICON_CACHE_LIMIT);
    expect(records?.has("site-0.example")).toBe(false);
    expect(records?.has("site-4.example")).toBe(false);
    expect(records?.has("site-5.example")).toBe(true);
    expect(records?.has(`site-${total - 1}.example`)).toBe(true);
  });

  it("works from memory when IndexedDB is unavailable", async () => {
    vi.stubGlobal("indexedDB", undefined);
    const cache = await loadCache();

    expect(await cache.loadFavicon("github.com")).toBe(ICON);
    expect(await cache.loadFavicon("github.com")).toBe(ICON);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(() => cache.clearFaviconCache()).not.toThrow();
  });

  it("survives a database that refuses to open", async () => {
    vi.stubGlobal("indexedDB", {
      open: () => {
        throw new Error("blocked");
      },
    } as unknown as IDBFactory);
    const cache = await loadCache();

    await expect(cache.loadFavicon("github.com")).resolves.toBe(ICON);
    await expect(cache.loadFavicon("github.com")).resolves.toBe(ICON);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("clears the database and the legacy localStorage blob", async () => {
    const cache = await loadCache();
    localStorage.setItem(
      cache.FAVICON_STORAGE_KEY,
      JSON.stringify({ version: 1, hosts: { "github.com": { at: 1 } } }),
    );
    await cache.loadFavicon("github.com");
    expect(fake.records()?.size).toBe(1);

    cache.clearFaviconCache();
    await flush();

    expect(localStorage.getItem(cache.FAVICON_STORAGE_KEY)).toBeNull();
    expect(fake.records()?.size).toBe(0);

    await expect(cache.loadFavicon("github.com")).resolves.toBe(ICON);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
});
