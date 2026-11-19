import { beforeEach, describe, expect, it, vi } from "vitest";
import { readSyncState, writeSyncState } from "./state";
import type { VaultStorage } from "@/lib/storage";

function memoryStorage(): VaultStorage {
  const files = new Map<string, Uint8Array>();
  return {
    readFile: async (path) => {
      const found = files.get(path);
      if (!found) throw { code: "storage", message: "нет файла" };
      return found;
    },
    saveFile: async (path, bytes) => {
      files.set(path, bytes);
    },
    vaultExists: async () => false,
  } as unknown as VaultStorage;
}

describe("sync state", () => {
  let storage: VaultStorage;

  beforeEach(() => {
    storage = memoryStorage();
  });

  it("starts from a clean state when the file is missing", async () => {
    await expect(readSyncState(storage, "/cache/p1")).resolves.toEqual({
      baseVersion: 0,
      dirty: false,
      lastSyncedAt: null,
      lastError: null,
    });
  });

  it("round-trips the state", async () => {
    await writeSyncState(storage, "/cache/p1", {
      baseVersion: 7,
      dirty: true,
      lastSyncedAt: "2026-11-19T10:00:00.000Z",
      lastError: null,
    });

    await expect(readSyncState(storage, "/cache/p1")).resolves.toEqual({
      baseVersion: 7,
      dirty: true,
      lastSyncedAt: "2026-11-19T10:00:00.000Z",
      lastError: null,
    });
  });

  it("does not fall over a corrupt file", async () => {
    const broken = {
      ...storage,
      readFile: vi.fn(async () => new TextEncoder().encode("{не json")),
    } as unknown as VaultStorage;

    await expect(readSyncState(broken, "/cache/p1")).resolves.toEqual({
      baseVersion: 0,
      dirty: false,
      lastSyncedAt: null,
      lastError: null,
    });
  });
});
