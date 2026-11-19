import { describe, expect, it, vi } from "vitest";
import { shouldDelete, syncBlobs, RETENTION_DAYS } from "./blobs";
import type { VaultStorage } from "@/lib/storage";
import type { ServerBlob } from "@/features/vault/types";

const NOW = new Date("2026-11-19T12:00:00.000Z");
const OLD = "2026-10-01T00:00:00.000Z";
const FRESH = "2026-11-18T00:00:00.000Z";

function blob(id: string, lastUsedAt: string): ServerBlob {
  return { blobId: id, size: 10, digest: "ab", createdAt: lastUsedAt, lastUsedAt };
}

function storageWith(localIds: string[]): VaultStorage {
  return {
    listBlobIds: async () => localIds,
  } as unknown as VaultStorage;
}

describe("blob sync", () => {
  it("never deletes a blob the server used recently", () => {
    expect(shouldDelete(blob("a", FRESH), new Set(), NOW)).toBe(false);
    expect(shouldDelete(blob("a", OLD), new Set(), NOW)).toBe(true);
    expect(RETENTION_DAYS).toBe(30);
  });

  it("never deletes a blob that exists locally", () => {
    expect(shouldDelete(blob("a", OLD), new Set(["a"]), NOW)).toBe(false);
  });

  it("uploads local blobs the server does not have", async () => {
    const upload = vi.fn(async () => blob("new", FRESH));
    const result = await syncBlobs({
      accountId: "ab",
      cacheDir: "/cache/p1",
      storage: storageWith(["new", "known"]),
      now: () => NOW,
      remote: {
        remoteListBlobs: vi.fn(async () => [blob("known", FRESH)]),
        remoteUploadBlob: upload,
        remoteDeleteBlob: vi.fn(async () => true),
      },
    });

    expect(result.uploaded).toEqual(["new"]);
    expect(upload).toHaveBeenCalledWith("ab", "new", "/cache/p1/attachments/new.pmsa");
  });

  it("deletes only old, unused server blobs", async () => {
    const remove = vi.fn(async () => true);
    const result = await syncBlobs({
      accountId: "ab",
      cacheDir: "/cache/p1",
      storage: storageWith([]),
      now: () => NOW,
      remote: {
        remoteListBlobs: vi.fn(async () => [blob("old", OLD), blob("fresh", FRESH)]),
        remoteUploadBlob: vi.fn(),
        remoteDeleteBlob: remove,
      },
    });

    expect(result.deleted).toEqual(["old"]);
    expect(remove).toHaveBeenCalledOnce();
  });

  it("keeps going when one blob fails", async () => {
    const result = await syncBlobs({
      accountId: "ab",
      cacheDir: "/cache/p1",
      storage: storageWith(["a", "b"]),
      now: () => NOW,
      remote: {
        remoteListBlobs: vi.fn(async () => []),
        remoteUploadBlob: vi.fn(async (_account: string, id: string) => {
          if (id === "a") throw { code: "remote_unreachable", message: "сеть" };
          return blob(id, FRESH);
        }),
        remoteDeleteBlob: vi.fn(async () => true),
      },
    });

    expect(result.uploaded).toEqual(["b"]);
    expect(result.failed).toEqual(["a"]);
  });
});
