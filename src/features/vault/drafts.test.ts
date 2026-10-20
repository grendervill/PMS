import { describe, expect, it } from "vitest";
import type { VaultData } from "@/features/vault/types";
import { defaultVaultSettings } from "@/features/vault/types";
import { findDraft, removeDraft, saveDraft } from "./drafts";

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
};

describe("drafts", () => {
  it("saves and updates a draft by id", () => {
    const first = saveDraft(
      data,
      { id: "new", kind: "entry", payload: { name: "Черновик" } },
      "2026-10-20T11:00:00.000Z",
    );
    const second = saveDraft(
      first,
      { id: "new", kind: "entry", payload: { name: "Обновлён" } },
      "2026-10-20T12:00:00.000Z",
    );

    expect(second.drafts).toHaveLength(1);
    expect(second.drafts[0]?.payload).toEqual({ name: "Обновлён" });
    expect(second.updatedAt).toBe("2026-10-20T12:00:00.000Z");
  });

  it("finds and removes a draft", () => {
    const withDraft = saveDraft(data, {
      id: "entry-1",
      kind: "entry",
      payload: { name: "GitHub" },
    });

    expect(findDraft(withDraft, "entry-1")?.kind).toBe("entry");
    expect(removeDraft(withDraft, "entry-1").drafts).toEqual([]);
  });
});
