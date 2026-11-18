/**
 * Minimal promise wrapper over IndexedDB for small key/value caches.
 *
 * The point is to avoid rewriting the whole cache on every change: one record
 * per key, one transaction per operation, no third-party library. Operations
 * are serialized, so `clear()` always lands before later reads/writes.
 * When IndexedDB is missing (private mode, old WebView) the factory returns
 * `null` and callers fall back to memory.
 */

export type KvStore<T> = {
  get(key: string): Promise<T | undefined>;
  put(key: string, value: T): Promise<void>;
  delete(key: string): Promise<void>;
  clear(): Promise<void>;
  keys(): Promise<string[]>;
};

const DATABASE_VERSION = 1;

function openDatabase(name: string, storeName: string): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(name, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(storeName)) {
        database.createObjectStore(storeName);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("indexeddb_open_failed"));
    request.onblocked = () => reject(new Error("indexeddb_blocked"));
  });
}

export function createKvStore<T>(databaseName: string, storeName = "records"): KvStore<T> | null {
  if (typeof indexedDB === "undefined" || indexedDB === null) return null;

  let opening: Promise<IDBDatabase> | null = null;
  let queue: Promise<unknown> = Promise.resolve();

  const database = (): Promise<IDBDatabase> => {
    if (!opening) {
      opening = openDatabase(databaseName, storeName);
      void opening.catch(() => {
        opening = null;
      });
    }
    return opening;
  };

  const run = async <R>(
    mode: IDBTransactionMode,
    action: (store: IDBObjectStore) => IDBRequest,
  ): Promise<R> => {
    const db = await database();
    return await new Promise<R>((resolve, reject) => {
      let request: IDBRequest;
      try {
        const transaction = db.transaction(storeName, mode);
        request = action(transaction.objectStore(storeName));
        transaction.oncomplete = () => resolve(request.result as R);
        transaction.onerror = () => reject(transaction.error ?? request.error ?? new Error("indexeddb_error"));
        transaction.onabort = () => reject(transaction.error ?? new Error("indexeddb_aborted"));
      } catch (error) {
        reject(error);
      }
    });
  };

  const enqueue = <R>(task: () => Promise<R>): Promise<R> => {
    const next = queue.then(task, task);
    queue = next.catch(() => undefined);
    return next;
  };

  return {
    get: (key) => enqueue(() => run<T | undefined>("readonly", (store) => store.get(key))),
    put: (key, value) =>
      enqueue(async () => {
        await run<unknown>("readwrite", (store) => store.put(value, key));
      }),
    delete: (key) =>
      enqueue(async () => {
        await run<unknown>("readwrite", (store) => store.delete(key));
      }),
    clear: () =>
      enqueue(async () => {
        await run<unknown>("readwrite", (store) => store.clear());
      }),
    keys: () =>
      enqueue(async () => {
        const keys = await run<IDBValidKey[]>("readonly", (store) => store.getAllKeys());
        return keys.map((key) => String(key));
      }),
  };
}
