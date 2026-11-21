import type { VaultStorage } from "@/lib/storage";
import type { ServerVaultMeta } from "@/features/vault/types";
import { readSyncState, writeSyncState, type SyncState } from "./state";

/** Ровно те сетевые вызовы, которые нужны движку. */
export type SyncRemote = {
  remoteVaultMeta(accountId: string): Promise<ServerVaultMeta | null>;
  remotePullVault(accountId: string, folder: string): Promise<ServerVaultMeta>;
  remotePushVault(
    accountId: string,
    folder: string,
    expectedVersion: number | null,
  ): Promise<ServerVaultMeta>;
};

export type SyncDeps = {
  accountId: string;
  cacheDir: string;
  storage: VaultStorage;
  remote: SyncRemote;
  now?: () => Date;
};

export type SyncOutcome =
  | { kind: "idle" }
  | { kind: "pulled"; version: number }
  | { kind: "pushed"; version: number }
  | { kind: "conflict"; remote: ServerVaultMeta }
  | { kind: "needs-auth" }
  | { kind: "offline"; message: string }
  | { kind: "error"; message: string };

export type SyncEngine = {
  sync(): Promise<SyncOutcome>;
  markDirty(): Promise<void>;
  state(): Promise<SyncState>;
  resolveKeepMine(): Promise<SyncOutcome>;
  resolveTakeTheirs(): Promise<SyncOutcome>;
};

function errorCode(error: unknown): string {
  if (error && typeof error === "object" && "code" in error) {
    return String((error as { code: unknown }).code);
  }
  return "unknown";
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function isOffline(error: unknown): boolean {
  const code = errorCode(error);
  return code === "remote_unreachable" || code === "remote_tls";
}

/** Сессии на сервере нет: после перезапуска приложения её надо получить заново. */
function isUnauthorized(error: unknown): boolean {
  return errorCode(error) === "remote_unauthorized";
}

export function createSyncEngine(deps: SyncDeps): SyncEngine {
  const { accountId, cacheDir, storage, remote } = deps;
  const now = () => (deps.now ? deps.now() : new Date()).toISOString();

  async function load(): Promise<SyncState> {
    return readSyncState(storage, cacheDir);
  }

  async function save(state: SyncState): Promise<void> {
    await writeSyncState(storage, cacheDir, state);
  }

  async function remoteMeta(): Promise<ServerVaultMeta | null | { error: SyncOutcome }> {
    try {
      return await remote.remoteVaultMeta(accountId);
    } catch (error) {
      if (isOffline(error)) return { error: { kind: "offline", message: errorMessage(error) } };
      if (isUnauthorized(error)) return { error: { kind: "needs-auth" } };
      return { error: { kind: "error", message: errorMessage(error) } };
    }
  }

  function isFailure(value: unknown): value is { error: SyncOutcome } {
    return Boolean(value) && typeof value === "object" && "error" in (value as object);
  }

  async function push(expected: number | null, state: SyncState, remoteVersion?: number) {
    try {
      const meta = await remote.remotePushVault(accountId, cacheDir, expected);
      await save({
        baseVersion: meta.version,
        dirty: false,
        lastSyncedAt: now(),
        lastError: null,
      });
      return { kind: "pushed", version: meta.version } as const;
    } catch (error) {
      if (errorCode(error) === "remote_conflict") {
        const fresh = await remoteMeta();
        if (!isFailure(fresh) && fresh) {
          await save({ ...state, lastError: null });
          return { kind: "conflict", remote: fresh } as const;
        }
        void remoteVersion;
      }
      if (isUnauthorized(error)) {
        await save({ ...state, lastError: "нужен вход на сервер" });
        return { kind: "needs-auth" } as const;
      }
      if (isOffline(error)) {
        await save({ ...state, lastError: errorMessage(error) });
        return { kind: "offline", message: errorMessage(error) } as const;
      }
      await save({ ...state, lastError: errorMessage(error) });
      return { kind: "error", message: errorMessage(error) } as const;
    }
  }

  async function pull(state: SyncState, version: number) {
    try {
      const meta = await remote.remotePullVault(accountId, cacheDir);
      await save({
        baseVersion: meta.version,
        dirty: false,
        lastSyncedAt: now(),
        lastError: null,
      });
      return { kind: "pulled", version: meta.version } as const;
    } catch (error) {
      if (isUnauthorized(error)) return { kind: "needs-auth" } as const;
      if (isOffline(error)) return { kind: "offline", message: errorMessage(error) } as const;
      const failed = { ...state, lastError: errorMessage(error) };
      await save(failed);
      return { kind: "error", message: errorMessage(error) } as const;
    }
    void version;
  }

  return {
    async state() {
      return load();
    },

    async markDirty() {
      const state = await load();
      await save({ ...state, dirty: true });
    },

    async sync() {
      const state = await load();
      const local = await storage.vaultExists(cacheDir);
      const meta = await remoteMeta();
      if (isFailure(meta)) {
        // Ошибку связи фиксируем в состоянии: интерфейс покажет её, а `dirty`
        // останется, чтобы изменения не потерялись.
        await save({
          ...state,
          lastError: meta.error.kind === "offline" ? meta.error.message : state.lastError,
        });
        return meta.error;
      }

      if (!meta) {
        // На сервере пусто: либо первая отправка, либо нечего делать.
        return local ? push(null, state) : { kind: "idle" };
      }

      if (!local) {
        // Кэша нет (новое устройство): забираем серверную версию.
        return pull(state, meta.version);
      }

      if (state.dirty) {
        if (meta.version > state.baseVersion) {
          return { kind: "conflict", remote: meta };
        }
        return push(state.baseVersion, state);
      }

      if (meta.version > state.baseVersion || meta.version < state.baseVersion) {
        return pull(state, meta.version);
      }
      return { kind: "idle" };
    },

    async resolveKeepMine() {
      const state = await load();
      const meta = await remoteMeta();
      if (isFailure(meta)) return meta.error;
      return push(meta ? meta.version : null, { ...state, dirty: true });
    },

    async resolveTakeTheirs() {
      const state = await load();
      const local = await storage.vaultExists(cacheDir);
      if (local) {
        // Локальную версию не выбрасываем: сначала копия в backups/.
        const bytes = await storage.readVault(cacheDir);
        const stamp = now().replace(/[-:.]/g, "");
        await storage.writeBackup(cacheDir, `${stamp}.vault`, bytes);
      }
      const meta = await remoteMeta();
      if (isFailure(meta)) return meta.error;
      if (!meta) {
        await save({ ...state, dirty: false, lastError: null });
        return { kind: "idle" };
      }
      return pull(state, meta.version);
    },
  };
}
