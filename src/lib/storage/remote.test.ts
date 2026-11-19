import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { remoteStorage } from "./remote";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/path", () => ({
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));

const invokeMock = vi.mocked(invoke);

describe("remote storage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("never opens a folder picker: the cache path comes from Rust", async () => {
    await expect(remoteStorage.pickFolder()).rejects.toMatchObject({
      code: "remote_cache_path",
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reads and writes the vault inside the cache directory", async () => {
    const bytes = new TextEncoder().encode("sealed");
    invokeMock.mockResolvedValueOnce(bytes.buffer);
    await remoteStorage.readVault("/tmp/cache/p1");

    expect(invokeMock).toHaveBeenCalledWith("storage_read_file", {
      path: "/tmp/cache/p1/pms.vault",
    });

    invokeMock.mockResolvedValueOnce(undefined);
    await remoteStorage.writeVault("/tmp/cache/p1", bytes);
    expect(invokeMock).toHaveBeenCalledWith(
      "storage_write_atomic",
      expect.objectContaining({ path: "/tmp/cache/p1/pms.vault", mode: 384 }),
    );
  });

  it("keeps backups and attachments next to the cache", async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    await remoteStorage.writeBackup("/tmp/cache/p1", "b.vault", new Uint8Array([1]));
    expect(invokeMock).toHaveBeenCalledWith(
      "storage_write_atomic",
      expect.objectContaining({ path: "/tmp/cache/p1/backups/b.vault" }),
    );

    invokeMock.mockResolvedValueOnce([{ name: "20260101.vault", isFile: true }]);
    await expect(remoteStorage.listBackups("/tmp/cache/p1")).resolves.toEqual([
      "20260101.vault",
    ]);
    expect(invokeMock).toHaveBeenCalledWith("storage_list_dir", {
      path: "/tmp/cache/p1/backups",
    });
  });

  it("reports the vault as missing before the first pull", async () => {
    invokeMock.mockResolvedValueOnce(false);
    await expect(remoteStorage.vaultExists("/tmp/cache/p1")).resolves.toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("storage_exists", {
      path: "/tmp/cache/p1/pms.vault",
    });
  });
});
