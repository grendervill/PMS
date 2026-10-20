import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopStorage } from "./desktop";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/path", () => ({
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(open);

describe("desktop storage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
  });

  it("picks a folder and returns null when cancelled", async () => {
    openMock.mockResolvedValueOnce("/vaults/personal");
    await expect(desktopStorage.pickFolder()).resolves.toBe("/vaults/personal");

    openMock.mockResolvedValueOnce(null);
    await expect(desktopStorage.pickFolder()).resolves.toBeNull();
  });

  it("reads the vault from the chosen folder", async () => {
    invokeMock.mockResolvedValueOnce({ b64: "aGVsbG8=" });

    const bytes = await desktopStorage.readVault("/vaults/personal");

    expect(new TextDecoder().decode(bytes)).toBe("hello");
    expect(invokeMock).toHaveBeenCalledWith("storage_read_file", {
      path: "/vaults/personal/pms.vault",
    });
  });

  it("writes the vault atomically with private permissions", async () => {
    invokeMock.mockResolvedValueOnce(undefined);

    await desktopStorage.writeVault("/vaults/personal", new TextEncoder().encode("hello"));

    expect(invokeMock).toHaveBeenCalledWith("storage_write_atomic", {
      path: "/vaults/personal/pms.vault",
      b64: "aGVsbG8=",
      mode: 0o600,
    });
  });

  it("lists only vault backups", async () => {
    invokeMock.mockResolvedValueOnce([
      { name: "pms-20261020.vault", isFile: true },
      { name: "notes.txt", isFile: true },
      { name: "nested", isFile: false },
    ]);

    await expect(desktopStorage.listBackups("/vaults/personal")).resolves.toEqual([
      "pms-20261020.vault",
    ]);
  });

  it("returns no backups when the directory is missing", async () => {
    invokeMock.mockRejectedValueOnce(new Error("missing"));

    await expect(desktopStorage.listBackups("/vaults/personal")).resolves.toEqual([]);
  });

  it("reads and writes attachment blobs", async () => {
    invokeMock.mockResolvedValueOnce({ b64: "YmxvYg==" });
    const blob = await desktopStorage.readBlob("/vaults/personal", "abc");
    expect(Array.from(blob)).toEqual(Array.from(new TextEncoder().encode("blob")));

    invokeMock.mockResolvedValueOnce(undefined);
    await desktopStorage.writeBlob("/vaults/personal", "abc", new Uint8Array([1, 2, 3]));

    expect(invokeMock).toHaveBeenLastCalledWith("storage_write_atomic", {
      path: "/vaults/personal/attachments/abc.pmsa",
      b64: "AQID",
      mode: 0o600,
    });
  });
});
