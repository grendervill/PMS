import { beforeEach, describe, expect, it, vi } from "vitest";
import { getStorage } from "@/lib/storage";
import type { VaultStorage } from "@/lib/storage";
import { createEntry } from "@/features/entries/entries";
import { vaultApi } from "@/features/vault/api";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { runImport } from "./importService";

vi.mock("@/lib/storage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/storage")>();
  return { ...actual, getStorage: vi.fn() };
});

function storage(): VaultStorage {
  return {
    pickFolder: vi.fn(),
    pickFile: vi.fn(),
    readFile: vi.fn(),
    vaultExists: vi.fn(),
    ensureLayout: vi.fn(),
    readVault: vi.fn(),
    writeVault: vi.fn(),
    writeBackup: vi.fn(),
    listBackups: vi.fn(),
    statVault: vi.fn(),
    exists: vi.fn(),
    mkdir: vi.fn(),
    saveFile: vi.fn(),
    readBlob: vi.fn(),
    writeBlob: vi.fn(),
    deleteBlob: vi.fn(),
  };
}

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: "2026-10-21T10:00:00.000Z",
  updatedAt: "2026-10-21T10:00:00.000Z",
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  attachments: [],
  drafts: [],
};

describe("importService", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(getStorage).mockReturnValue(storage());
  });

  it("imports entries and seals attachments", async () => {
    vi.spyOn(vaultApi, "sealAttachment").mockResolvedValue({
      blob: new Uint8Array([1]),
      meta: {
        id: "att-1",
        name: "doc.txt",
        mime: "text/plain",
        size: 5,
        sha256: "hash",
        kind: "text",
        createdAt: "2026-10-21T10:00:00.000Z",
      },
    });
    const bundle = {
      format: "csv",
      warnings: [],
      items: [
        {
          kind: "login" as const,
          name: "GitHub",
          login: "alex",
          password: "s3cret",
          urls: ["https://github.com"],
          comment: "note",
          totp: null,
          attachments: [{ name: "doc.txt", mime: "text/plain", b64: "aGVsbG8=" }],
          sshKey: null,
        },
      ],
    };

    const result = await runImport(bundle, data, "session", "/vault", {
      skipDuplicates: true,
    });

    expect(result.result.entries).toBe(1);
    expect(result.result.attachments).toBe(1);
    expect(result.data.entries[0]?.name).toBe("GitHub");
    expect(result.data.entries[0]?.attachmentIds).toEqual(["att-1"]);
    expect(vaultApi.sealAttachment).toHaveBeenCalled();
  });

  it("skips exact duplicates by default", async () => {
    const existing = createEntry({
      name: "GitHub",
      login: "alex",
      password: "s3cret",
      urls: ["https://github.com"],
      comment: "",
      attachmentIds: [],
      sshKeyIds: [],
      totp: null,
      totpRaw: "",
    });
    const bundle = {
      format: "csv",
      warnings: [],
      items: [
        {
          kind: "login" as const,
          name: "GitHub",
          login: "alex",
          password: "s3cret",
          urls: ["https://github.com"],
          comment: "",
          totp: null,
          attachments: [],
          sshKey: null,
        },
      ],
    };

    const result = await runImport(
      bundle,
      { ...data, entries: [existing] },
      "session",
      "/vault",
      { skipDuplicates: true },
    );

    expect(result.result.skipped).toBe(1);
    expect(result.data.entries).toHaveLength(1);
  });
});
