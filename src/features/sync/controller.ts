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
  | { kind: "needs-auth" }
  | { kind: "error"; message: string };

export type SyncController = {
  stop(): void;
  markDirty(): void;
  resumeAfterAuth(): Promise<SyncOutcome | null>;
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
  let paused = false;
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
        case "needs-auth":
          // Без сессии сервера опрос бессмысленен: ждём разблокировки сейфа,
          // после которой приложение входит заново.
          paused = true;
          stopPeriod();
          report({ kind: "needs-auth" });
          break;
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
    if (paused) return;
    void run();
  };

  // В фоне (свёрнутое приложение, погашенный экран телефона) таймер не нужен:
  // будильник разбудит устройство зря. Синхронизируемся при возврате.
  let period: ReturnType<typeof setInterval> | null = null;
  const startPeriod = () => {
    if (period === null) {
      period = setInterval(() => {
        void run();
      }, PERIOD_MS);
    }
  };
  const stopPeriod = () => {
    if (period !== null) clearInterval(period);
    period = null;
  };
  startPeriod();

  const onVisibility = () => {
    if (typeof document === "undefined") return;
    if (document.visibilityState === "visible") {
      startPeriod();
      void run();
    } else {
      stopPeriod();
    }
  };

  if (typeof window !== "undefined") {
    window.addEventListener("focus", onFocus);
  }
  if (typeof document !== "undefined") {
    document.addEventListener("visibilitychange", onVisibility);
  }

  return {
    markDirty() {
      if (stopped) return;
      void engine.markDirty();
      // Без сессии сервера не долбим сеть: изменения подхватит первая
      // синхронизация после повторного входа.
      if (paused) return;
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

    async resumeAfterAuth() {
      paused = false;
      startPeriod();
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
      stopPeriod();
      if (typeof window !== "undefined") {
        window.removeEventListener("focus", onFocus);
      }
      if (typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", onVisibility);
      }
    },
  };
}
