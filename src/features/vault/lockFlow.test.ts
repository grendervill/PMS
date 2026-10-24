import { describe, expect, it, vi } from "vitest";
import type { VaultData } from "@/features/vault/types";
import { defaultVaultSettings } from "@/features/vault/types";
import { saveDraftBeforeLock } from "./lockFlow";

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: "2026-10-20T10:00:00.000Z",
  updatedAt: "2026-10-20T10:00:00.000Z",
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  attachments: [],
  drafts: [],
  bankCards: [],
  documents: [],
};

describe("saveDraftBeforeLock", () => {
  it("saves the draft before locking", async () => {
    const save = vi.fn().mockResolvedValue(true);
    const lock = vi.fn().mockResolvedValue(undefined);

    const ok = await saveDraftBeforeLock({
      snapshot: { id: "new", kind: "entry", payload: { name: "Черновик" } },
      getData: () => data,
      save,
      lock,
    });

    expect(ok).toBe(true);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0]?.[0].drafts).toHaveLength(1);
    expect(lock).toHaveBeenCalledTimes(1);
  });

  it("does not lock when the draft cannot be saved", async () => {
    const save = vi.fn().mockResolvedValue(false);
    const lock = vi.fn().mockResolvedValue(undefined);

    const ok = await saveDraftBeforeLock({
      snapshot: { id: "new", kind: "entry", payload: { name: "Черновик" } },
      getData: () => data,
      save,
      lock,
    });

    expect(ok).toBe(false);
    expect(lock).not.toHaveBeenCalled();
  });

  it("locks without writing when there is no draft", async () => {
    const save = vi.fn().mockResolvedValue(true);
    const lock = vi.fn().mockResolvedValue(undefined);

    const ok = await saveDraftBeforeLock({
      snapshot: null,
      getData: () => data,
      save,
      lock,
    });

    expect(ok).toBe(true);
    expect(save).not.toHaveBeenCalled();
    expect(lock).toHaveBeenCalledTimes(1);
  });
});
