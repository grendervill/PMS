import { describe, expect, it, vi } from "vitest";
import { createSyncEngine, type SyncRemote } from "./engine";
import type { VaultStorage } from "@/lib/storage";
import type { ServerVaultMeta } from "@/features/vault/types";
import { readSyncState, writeSyncState } from "./state";

type Memory = VaultStorage & { files: Map<string, Uint8Array>; removed: string[] };

function memoryStorage(initial?: Uint8Array): Memory {
  const files = new Map<string, Uint8Array>();
  if (initial) files.set("/cache/p1/pms.vault", initial);
  const storage = {
    files,
    removed: [] as string[],
    readFile: async (path: string) => {
      const found = files.get(path);
      if (!found) throw { code: "storage", message: "нет файла" };
      return found;
    },
    saveFile: async (path: string, bytes: Uint8Array) => {
      files.set(path, bytes);
    },
    writeVault: async (_folder: string, bytes: Uint8Array) => {
      files.set("/cache/p1/pms.vault", bytes);
    },
    readVault: async () => files.get("/cache/p1/pms.vault") ?? new Uint8Array(),
    vaultExists: async () => files.has("/cache/p1/pms.vault"),
    writeBackup: async (folder: string, name: string, bytes: Uint8Array) => {
      files.set(`${folder}/backups/${name}`, bytes);
    },
  } as unknown as Memory;
  return storage;
}

function meta(version: number): ServerVaultMeta {
  return { version, size: 10, digest: "ab", updatedAt: "2026-11-19T10:00:00Z" };
}

function remoteStub(overrides: Partial<SyncRemote> = {}): SyncRemote & {
  push: ReturnType<typeof vi.fn>;
  pull: ReturnType<typeof vi.fn>;
} {
  const push = vi.fn(async (_account: string, _dir: string, expected: number | null) => ({
    ...meta((expected ?? 0) + 1),
  }));
  const pull = vi.fn(async () => meta(2));
  return {
    remoteVaultMeta: vi.fn(async () => meta(1)),
    remotePullVault: pull,
    remotePushVault: push,
    push,
    pull,
    ...overrides,
  } as SyncRemote & { push: ReturnType<typeof vi.fn>; pull: ReturnType<typeof vi.fn> };
}

function engineWith(remote: SyncRemote, storage: Memory) {
  return createSyncEngine({
    accountId: "ab".repeat(16),
    cacheDir: "/cache/p1",
    storage,
    remote,
    now: () => new Date("2026-11-19T12:00:00.000Z"),
  });
}

describe("sync engine", () => {
  it("does nothing when the versions already match", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: false,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub();
    const outcome = await engineWith(remote, storage).sync();

    expect(outcome).toEqual({ kind: "idle" });
    expect(remote.push).not.toHaveBeenCalled();
    expect(remote.pull).not.toHaveBeenCalled();
  });

  it("pulls when the server is ahead and locally clean", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: false,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub({ remoteVaultMeta: vi.fn(async () => meta(2)) });
    const outcome = await engineWith(remote, storage).sync();

    expect(outcome).toEqual({ kind: "pulled", version: 2 });
    expect(remote.pull).toHaveBeenCalledOnce();
    expect((await readSyncState(storage, "/cache/p1")).baseVersion).toBe(2);
  });

  it("pushes local changes with the base version", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: true,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub();
    const outcome = await engineWith(remote, storage).sync();

    expect(outcome).toEqual({ kind: "pushed", version: 2 });
    expect(remote.push).toHaveBeenCalledWith("ab".repeat(16), "/cache/p1", 1);
    expect(await readSyncState(storage, "/cache/p1")).toMatchObject({
      baseVersion: 2,
      dirty: false,
    });
  });

  it("reports a conflict instead of overwriting", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: true,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub({ remoteVaultMeta: vi.fn(async () => meta(3)) });
    const outcome = await engineWith(remote, storage).sync();

    expect(outcome).toMatchObject({ kind: "conflict" });
    expect(remote.push).not.toHaveBeenCalled();
  });

  it("keeps the dirty flag when the server is unreachable", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: true,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub({
      remoteVaultMeta: vi.fn(async () => {
        throw { code: "remote_unreachable", message: "нет сети" };
      }),
    });

    const outcome = await engineWith(remote, storage).sync();

    expect(outcome.kind).toBe("offline");
    const state = await readSyncState(storage, "/cache/p1");
    expect(state.dirty).toBe(true);
    expect(state.lastError).toContain("нет сети");
  });

  it("keep-mine pushes over the remote version", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: true,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub({ remoteVaultMeta: vi.fn(async () => meta(3)) });
    const engine = engineWith(remote, storage);

    const outcome = await engine.resolveKeepMine();

    expect(outcome).toEqual({ kind: "pushed", version: 4 });
    expect(remote.push).toHaveBeenCalledWith("ab".repeat(16), "/cache/p1", 3);
  });

  it("take-theirs keeps a local backup before pulling", async () => {
    const storage = memoryStorage(new Uint8Array([9, 9, 9]));
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 1,
      dirty: true,
      lastSyncedAt: null,
      lastError: null,
    });
    const remote = remoteStub({ remoteVaultMeta: vi.fn(async () => meta(3)) });
    const engine = engineWith(remote, storage);

    const outcome = await engine.resolveTakeTheirs();

    expect(outcome.kind).toBe("pulled");
    const backups = [...storage.files.keys()].filter((key) => key.includes("/backups/"));
    expect(backups).toHaveLength(1);
    expect(remote.pull).toHaveBeenCalledOnce();
  });

  it("first push of a brand-new server vault uses no precondition", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    const remote = remoteStub({ remoteVaultMeta: vi.fn(async () => null) });
    const outcome = await engineWith(remote, storage).sync();

    expect(outcome).toEqual({ kind: "pushed", version: 1 });
    expect(remote.push).toHaveBeenCalledWith("ab".repeat(16), "/cache/p1", null);
  });
});

describe("sync engine without a session", () => {
  it("asks for a login instead of a generic error", async () => {
    const storage = memoryStorage(new Uint8Array([1]));
    const remote = remoteStub({
      remoteVaultMeta: vi.fn(async () => {
        throw { code: "remote_unauthorized", message: "нужен вход" };
      }),
    });

    const outcome = await engineWith(remote, storage).sync();
    expect(outcome).toEqual({ kind: "needs-auth" });
  });
});
