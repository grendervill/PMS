import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { startSync, type SyncStatus } from "./controller";
import type { VaultStorage } from "@/lib/storage";
import type { SyncEngine } from "./engine";

function storage(): VaultStorage {
  return {
    vaultExists: async () => true,
    listBlobIds: async () => [],
  } as unknown as VaultStorage;
}

function engineStub(outcome: Awaited<ReturnType<SyncEngine["sync"]>>): SyncEngine {
  return {
    sync: vi.fn(async () => outcome),
    markDirty: vi.fn(async () => undefined),
    state: vi.fn(async () => ({
      baseVersion: 1,
      dirty: false,
      lastSyncedAt: null,
      lastError: null,
    })),
    resolveKeepMine: vi.fn(async () => outcome),
    resolveTakeTheirs: vi.fn(async () => outcome),
  };
}

function deps(engine: SyncEngine, statuses: SyncStatus[]) {
  return {
    accountId: "ab".repeat(16),
    cacheDir: "/cache/p1",
    storage: storage(),
    engine,
    remote: {
      remoteListBlobs: vi.fn(async () => []),
      remoteUploadBlob: vi.fn(),
      remoteDeleteBlob: vi.fn(),
    },
    onStatus: (status: SyncStatus) => statuses.push(status),
  };
}

describe("sync controller", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("coalesces rapid saves into one push", async () => {
    const engine = engineStub({ kind: "pushed", version: 2 });
    const statuses: SyncStatus[] = [];
    const sync = startSync(deps(engine, statuses));

    sync.markDirty();
    await vi.advanceTimersByTimeAsync(500);
    sync.markDirty();
    await vi.advanceTimersByTimeAsync(2000);

    expect(engine.sync).toHaveBeenCalledOnce();
    expect(statuses.at(-1)).toEqual({ kind: "idle" });
    sync.stop();
  });

  it("reports a conflict and does not retry by itself", async () => {
    const engine = engineStub({
      kind: "conflict",
      remote: { version: 5, size: 1, digest: "ab", updatedAt: "" },
    });
    const statuses: SyncStatus[] = [];
    const sync = startSync(deps(engine, statuses));

    await sync.syncNow();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(statuses.at(-1)).toEqual({ kind: "conflict", remoteVersion: 5 });
    expect(engine.sync).toHaveBeenCalledOnce();
    sync.stop();
  });

  it("keeps the dirty flag when offline", async () => {
    const engine = engineStub({ kind: "offline", message: "нет сети" });
    const statuses: SyncStatus[] = [];
    const sync = startSync(deps(engine, statuses));

    await sync.syncNow();

    expect(statuses.at(-1)).toEqual({ kind: "offline", message: "нет сети" });
    sync.stop();
  });

  it("syncs on window focus", async () => {
    const engine = engineStub({ kind: "pulled", version: 3 });
    const statuses: SyncStatus[] = [];
    const sync = startSync(deps(engine, statuses));

    window.dispatchEvent(new Event("focus"));
    await vi.advanceTimersByTimeAsync(0);

    expect(engine.sync).toHaveBeenCalledOnce();
    sync.stop();
  });

  it("does not start a second sync while one is running", async () => {
    let resolve!: (value: Awaited<ReturnType<SyncEngine["sync"]>>) => void;
    const engine = engineStub({ kind: "idle" });
    engine.sync = vi.fn(
      () =>
        new Promise<Awaited<ReturnType<SyncEngine["sync"]>>>((done) => {
          resolve = done;
        }),
    );
    const sync = startSync(deps(engine, []));

    const first = sync.syncNow();
    void sync.syncNow();
    resolve({ kind: "idle" });
    await first;

    expect(engine.sync).toHaveBeenCalledOnce();
    sync.stop();
  });
});
