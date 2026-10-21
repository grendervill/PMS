import { beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage } from "@/lib/storage";
import type { VaultStorage } from "@/lib/storage";
import { defaultVaultSettings } from "@/features/vault/types";
import { backupDate, backupName, cleanupBackups, createBackup } from "./backups";

vi.mock("@/lib/storage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/storage")>();
  return { ...actual, getStorage: vi.fn() };
});

const now = new Date("2026-10-21T12:00:00.000Z");

function storage(): VaultStorage {
  return {
    pickFolder: vi.fn(),
    pickFile: vi.fn(),
    pickSaveFile: vi.fn(),
    displayName: vi.fn((value: string) => value.split("/").at(-1) ?? "Хранилище"),
    readFile: vi.fn(),
    vaultExists: vi.fn(),
    ensureLayout: vi.fn(),
    readVault: vi.fn().mockResolvedValue(new Uint8Array([1, 2])),
    writeVault: vi.fn(),
    writeBackup: vi.fn(),
    listBackups: vi.fn().mockResolvedValue([]),
    deleteBackup: vi.fn(),
    statVault: vi.fn(),
    exists: vi.fn(),
    mkdir: vi.fn(),
    saveFile: vi.fn(),
    readBlob: vi.fn(),
    writeBlob: vi.fn(),
    listBlobIds: vi.fn().mockResolvedValue([]),
    deleteBlob: vi.fn(),
  };
}

describe("backups", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    vi.mocked(getStorage).mockReturnValue(storage());
  });

  it("encodes and parses backup names", () => {
    const name = backupName(now);
    expect(name).toBe("20261021T120000000Z.vault");
    expect(backupDate(name)?.toISOString()).toBe(now.toISOString());
  });

  it("creates an auto backup when the newest one is old", async () => {
    const store = storage();
    vi.mocked(getStorage).mockReturnValue(store);
    vi.mocked(store.listBackups).mockResolvedValue(["20261019T120000000Z.vault"]);
    expect(await createBackup("/vault", { ...defaultVaultSettings(), autoBackupDays: 1 })).toBe(
      true,
    );
    expect(store.writeBackup).toHaveBeenCalled();
  });

  it("does not create an auto backup within the interval", async () => {
    const store = storage();
    vi.mocked(getStorage).mockReturnValue(store);
    vi.mocked(store.listBackups).mockResolvedValue([backupName(now)]);
    expect(await createBackup("/vault", { ...defaultVaultSettings(), autoBackupDays: 7 })).toBe(
      false,
    );
    expect(store.writeBackup).not.toHaveBeenCalled();
  });

  it("keeps the newest backup when cleaning old ones", async () => {
    const store = storage();
    vi.mocked(getStorage).mockReturnValue(store);
    vi.mocked(store.listBackups).mockResolvedValue([
      "20261021T120000000Z.vault",
      "20251021T120000000Z.vault",
    ]);
    expect(await cleanupBackups("/vault", 7)).toBe(1);
    expect(store.deleteBackup).toHaveBeenCalledWith("/vault", "20251021T120000000Z.vault");
  });
});
