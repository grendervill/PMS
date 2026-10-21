import { beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage } from "@/lib/storage";
import { vaultApi } from "@/features/vault/api";
import { exportVaultArchive, importVaultArchive } from "@/lib/storage/archive";

vi.mock("@/lib/storage", () => ({
  getStorage: vi.fn(),
}));

vi.mock("@/lib/platform", () => ({
  isAndroidPlatform: () => false,
}));

const storage = {
  pickSaveFile: vi.fn(),
  displayName: vi.fn(),
  saveFile: vi.fn(),
  readFile: vi.fn(),
  readVault: vi.fn(),
  writeVault: vi.fn(),
  listBlobIds: vi.fn(),
  readBlob: vi.fn(),
  writeBlob: vi.fn(),
  writeBackup: vi.fn(),
  vaultExists: vi.fn(),
  ensureLayout: vi.fn(),
};

describe("vault archive helpers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getStorage).mockReturnValue(storage as never);
  });

  it("exports to the selected desktop path", async () => {
    storage.pickSaveFile.mockResolvedValue("/tmp/pms.pmsvault");
    const exportSpy = vi.spyOn(vaultApi, "pmsExport").mockResolvedValue();

    await expect(exportVaultArchive("/vault", "pms.pmsvault")).resolves.toBe("/tmp/pms.pmsvault");
    expect(exportSpy).toHaveBeenCalledWith("/vault", "/tmp/pms.pmsvault");
  });

  it("imports into the selected desktop destination", async () => {
    const importSpy = vi.spyOn(vaultApi, "pmsImport").mockResolvedValue("/restored");

    await expect(importVaultArchive("/tmp/pms.pmsvault", "/restored")).resolves.toBe("/restored");
    expect(importSpy).toHaveBeenCalledWith("/tmp/pms.pmsvault", "/restored");
  });
});
