import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { desktopStorage } from "./desktop";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/path", () => ({
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));

const invokeMock = vi.mocked(invoke);

function bytesOf(value: string): ArrayBuffer {
  const bytes = new TextEncoder().encode(value);
  return bytes.buffer.slice(0, bytes.byteLength) as ArrayBuffer;
}

describe("desktop storage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("picks a folder through the Rust dialog", async () => {
    // The dialog lives in Rust because it is also what grants filesystem access.
    invokeMock.mockResolvedValueOnce("/vaults/personal");
    await expect(desktopStorage.pickFolder()).resolves.toBe("/vaults/personal");

    invokeMock.mockResolvedValueOnce(null);
    await expect(desktopStorage.pickFolder()).resolves.toBeNull();
    expect(invokeMock).toHaveBeenCalledWith("dialog_pick_folder", expect.anything());
  });

  it("reads the vault from the chosen folder as raw bytes", async () => {
    invokeMock.mockResolvedValueOnce(bytesOf("hello"));

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
    invokeMock.mockResolvedValueOnce(bytesOf("blob"));
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

  it("seals attachments through Rust and opens them as raw bytes", async () => {
    const meta = { id: "att-1", name: "a.txt", size: 3 };
    invokeMock.mockResolvedValueOnce({ meta });
    await expect(
      desktopStorage.sealAttachment("s1", "/vaults/personal", "a.txt", "text/plain", new Uint8Array([1, 2, 3])),
    ).resolves.toEqual(meta);
    expect(invokeMock).toHaveBeenLastCalledWith("attachment_seal_store", {
      sessionId: "s1",
      folder: "/vaults/personal",
      name: "a.txt",
      mime: "text/plain",
      b64: "AQID",
    });

    invokeMock.mockResolvedValueOnce(bytesOf("plain"));
    const opened = await desktopStorage.openAttachment("s1", "/vaults/personal", "att-1");
    expect(new TextDecoder().decode(opened)).toBe("plain");
    expect(invokeMock).toHaveBeenLastCalledWith("attachment_open_file", {
      sessionId: "s1",
      attachmentId: "att-1",
      folder: "/vaults/personal",
    });
  });
});
