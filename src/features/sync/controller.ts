import type { VaultStorage } from "@/lib/storage";
import { syncBlobs, type BlobRemote } from "./blobs";
import type { SyncEngine, SyncOutcome } from "./engine";

const DEBOUNCE_MS = 1500;
const PERIOD_MS = 5 * 60 * 1000;

export type SyncStatus =
  | { kind: "idle" }
  | { kind: "syncing" }
  | { kind: "dirty" }
  | { kind: "offline"; message: string }
  | { kind: "conflict"; remoteVersion: number }
  | { kind: "error"; message: string };

export type SyncController = {
  stop(): void;
  markDirty(): void;
  syncNow(): Promise<SyncOutcome | null>;
  resolveKeepMine(): Promise<SyncOutcome>;
  resolveTakeTheirs(): Promise<SyncOutcome>;
};

type Options = {
  accountId: string;
  cacheDir: string;
  storage: VaultStorage;
  engine: SyncEngine;
  remote: BlobRemote;
  onStatus(status: SyncStatus): void;
};

/**
 * Расписание синхронизации: после сохранений (с дебаунсом), по фокусу окна и
 * раз в пять минут. Одновременно идёт не больше одной синхронизации: второй
 * вызов ждёт первого, а не создаёт дубль запроса.
 */
export function startSync(options: Options): SyncController {
  const { accountId, cacheDir, storage, engine, remote, onStatus } = options;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let running: Promise<SyncOutcome | null> | null = null;

  const report = (status: SyncStatus) => {
    if (!stopped) onStatus(status);
  };

  async function run(): Promise<SyncOutcome | null> {
    if (running) return running;
    running = (async () => {
      report({ kind: "syncing" });
      const outcome = await engine.sync();

      if (outcome.kind === "pushed" || outcome.kind === "pulled" || outcome.kind === "idle") {
        try {
          await syncBlobs({ accountId, cacheDir, storage, remote });
        } catch {
          // Вложения догонят в следующий раз: сейф уже синхронизирован.
        }
      }

      switch (outcome.kind) {
        case "conflict":
          report({ kind: "conflict", remoteVersion: outcome.remote.version });
          break;
        case "offline":
          report({ kind: "offline", message: outcome.message });
          break;
        case "error":
          report({ kind: "error", message: outcome.message });
          break;
        default:
          report({ kind: "idle" });
      }
      return outcome;
    })().finally(() => {
      running = null;
    });
    return running;
  }

  const onFocus = () => {
    void run();
  };
  const period = setInterval(() => {
    void run();
  }, PERIOD_MS);

  if (typeof window !== "undefined") {
    window.addEventListener("focus", onFocus);
  }

  return {
    markDirty() {
      if (stopped) return;
      void engine.markDirty();
      report({ kind: "dirty" });
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        void run();
      }, DEBOUNCE_MS);
    },

    async syncNow() {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      return run();
    },

    async resolveKeepMine() {
      const outcome = await engine.resolveKeepMine();
      if (outcome.kind === "pushed" || outcome.kind === "idle") report({ kind: "idle" });
      else if (outcome.kind === "offline") report({ kind: "offline", message: outcome.message });
      else if (outcome.kind === "error") report({ kind: "error", message: outcome.message });
      return outcome;
    },

    async resolveTakeTheirs() {
      const outcome = await engine.resolveTakeTheirs();
      if (outcome.kind === "pulled" || outcome.kind === "idle") report({ kind: "idle" });
      else if (outcome.kind === "offline") report({ kind: "offline", message: outcome.message });
      else if (outcome.kind === "error") report({ kind: "error", message: outcome.message });
      return outcome;
    },

    stop() {
      stopped = true;
      if (timer) clearTimeout(timer);
      clearInterval(period);
      if (typeof window !== "undefined") {
        window.removeEventListener("focus", onFocus);
      }
    },
  };
}
