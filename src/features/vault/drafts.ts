import type { Draft, VaultData } from "@/features/vault/types";

export type DraftSnapshot = {
  id: string;
  kind: "entry" | "ssh";
  payload: unknown;
};

export function saveDraft(
  data: VaultData,
  snapshot: DraftSnapshot,
  now = new Date().toISOString(),
): VaultData {
  const draft: Draft = {
    id: snapshot.id,
    kind: snapshot.kind,
    payload: snapshot.payload,
    updatedAt: now,
  };

  return {
    ...data,
    drafts: [draft, ...data.drafts.filter((item) => item.id !== snapshot.id)],
    updatedAt: now,
  };
}

export function removeDraft(data: VaultData, id: string): VaultData {
  return {
    ...data,
    drafts: data.drafts.filter((item) => item.id !== id),
    updatedAt: new Date().toISOString(),
  };
}

export function findDraft(data: VaultData, id: string): Draft | undefined {
  return data.drafts.find((item) => item.id === id);
}
