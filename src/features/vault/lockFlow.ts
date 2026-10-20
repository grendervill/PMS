import { saveDraft, type DraftSnapshot } from "@/features/vault/drafts";
import type { VaultData } from "@/features/vault/types";

export async function saveDraftBeforeLock({
  snapshot,
  getData,
  save,
  lock,
}: {
  snapshot: DraftSnapshot | null;
  getData: () => VaultData | null;
  save: (data: VaultData) => Promise<boolean>;
  lock: () => Promise<void>;
}): Promise<boolean> {
  if (snapshot) {
    const data = getData();
    if (!data) return false;
    const saved = await save(saveDraft(data, snapshot));
    if (!saved) return false;
  }

  await lock();
  return true;
}
