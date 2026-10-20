import { create } from "zustand";
import { addRecent, getStorage, listRecent } from "@/lib/storage";
import type { RecentVault } from "@/lib/storage";
import { VaultApiError, vaultApi } from "@/features/vault/api";
import {
  createEntry,
  removeEntry,
  updateEntry as applyEntryUpdate,
  type EntryDraft,
} from "@/features/entries/entries";
import { removeDraft as removeDraftFromData } from "@/features/vault/drafts";
import type {
  CreateVaultInput,
  UnlockSecret,
  VaultData,
  VaultErrorInfo,
  VaultPublicInfo,
  VaultStatus,
} from "@/features/vault/types";

export type VaultStateData = {
  status: VaultStatus;
  initialized: boolean;
  busy: boolean;
  sessionId: string | null;
  vaultPath: string | null;
  publicInfo: VaultPublicInfo | null;
  data: VaultData | null;
  recent: RecentVault[];
  error: VaultErrorInfo | null;
};

export type VaultActions = {
  init: () => Promise<void>;
  createVault: (input: CreateVaultInput) => Promise<boolean>;
  openVault: (path: string) => Promise<boolean>;
  unlock: (secret: UnlockSecret) => Promise<boolean>;
  save: (data: VaultData) => Promise<boolean>;
  addEntry: (draft: EntryDraft) => Promise<boolean>;
  updateEntry: (
    id: string,
    draft: EntryDraft,
  ) => Promise<{ ok: boolean; blockedReason?: "old-password" }>;
  deleteEntry: (id: string) => Promise<boolean>;
  removeDraft: (id: string) => Promise<boolean>;
  lock: () => Promise<void>;
  reset: () => Promise<void>;
  clearError: () => void;
};

export type VaultStore = VaultStateData & VaultActions;

export const initialVaultState: VaultStateData = {
  status: "welcome",
  initialized: false,
  busy: false,
  sessionId: null,
  vaultPath: null,
  publicInfo: null,
  data: null,
  recent: [],
  error: null,
};

function toErrorInfo(error: unknown): VaultErrorInfo {
  if (error instanceof VaultApiError) {
    return { code: error.code, message: error.message };
  }
  if (error && typeof error === "object" && "code" in error) {
    const candidate = error as Partial<VaultErrorInfo>;
    if (typeof candidate.code === "string") {
      return {
        code: candidate.code,
        message: candidate.message ?? "Не удалось выполнить операцию",
      };
    }
  }
  return { code: "unknown", message: "Не удалось выполнить операцию" };
}

function baseName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? "Хранилище";
}

function recentEntry(path: string, name: string, vaultId: string): RecentVault {
  return {
    path,
    name,
    vaultId,
    lastOpenedAt: new Date().toISOString(),
  };
}

export const useVaultStore = create<VaultStore>((set, get) => ({
  ...initialVaultState,

  async init() {
    if (get().initialized) return;
    set({ initialized: true });
    const recent = listRecent();
    set({ recent });

    if (recent.length > 0) {
      const first = recent[0];
      if (first) await get().openVault(first.path);
    }
  },

  async createVault(input) {
    set({ busy: true, error: null });
    let createdSession: string | null = null;
    try {
      const storage = getStorage();
      if (await storage.vaultExists(input.folder)) {
        throw new VaultApiError("vault_exists", "В этой папке уже есть хранилище");
      }

      await storage.ensureLayout(input.folder);
      const created = await vaultApi.create(
        {
          name: input.name,
          kdfProfile: input.kdfProfile,
          cipher: input.cipher,
        },
        input.primarySecret,
      );
      createdSession = created.sessionId;

      let file = created.file;
      if (input.secondarySecret) {
        file = await vaultApi.rewrap(created.sessionId, input.secondarySecret, true);
      }

      const info = await vaultApi.inspect(file);
      await storage.writeVault(input.folder, file);
      const recent = addRecent(recentEntry(input.folder, created.data.name, created.data.id));

      set({
        status: "unlocked",
        sessionId: created.sessionId,
        vaultPath: input.folder,
        publicInfo: info,
        data: created.data,
        recent,
        busy: false,
        error: null,
      });
      return true;
    } catch (error) {
      if (createdSession) {
        await vaultApi.lock(createdSession).catch(() => undefined);
      }
      set({ busy: false, error: toErrorInfo(error) });
      return false;
    }
  },

  async openVault(path) {
    set({ busy: true, error: null });
    try {
      const file = await getStorage().readVault(path);
      const info = await vaultApi.inspect(file);
      const recent = addRecent(recentEntry(path, baseName(path), info.vaultId));

      set({
        status: "locked",
        vaultPath: path,
        publicInfo: info,
        sessionId: null,
        data: null,
        recent,
        busy: false,
        error: null,
      });
      return true;
    } catch (error) {
      set({
        status: "welcome",
        vaultPath: null,
        publicInfo: null,
        sessionId: null,
        data: null,
        recent: listRecent(),
        busy: false,
        error: toErrorInfo(error),
      });
      return false;
    }
  },

  async unlock(secret) {
    const path = get().vaultPath;
    if (!path) return false;

    set({ status: "unlocking", busy: true, error: null });
    try {
      const file = await getStorage().readVault(path);
      const unlocked = await vaultApi.unlock(file, secret);
      const info = await vaultApi.inspect(file);
      const recent = addRecent(
        recentEntry(path, unlocked.data.name, unlocked.data.id),
      );

      set({
        status: "unlocked",
        sessionId: unlocked.sessionId,
        data: unlocked.data,
        publicInfo: info,
        recent,
        busy: false,
        error: null,
      });
      return true;
    } catch (error) {
      set({
        status: "locked",
        sessionId: null,
        data: null,
        busy: false,
        error: toErrorInfo(error),
      });
      return false;
    }
  },

  async save(data) {
    const { sessionId, vaultPath } = get();
    if (!sessionId || !vaultPath) return false;

    set({ busy: true, error: null });
    try {
      const file = await vaultApi.save(sessionId, data);
      await getStorage().writeVault(vaultPath, file);
      set({ data, busy: false, error: null });
      return true;
    } catch (error) {
      set({ busy: false, error: toErrorInfo(error) });
      return false;
    }
  },

  async addEntry(draft) {
    const data = get().data;
    if (!data) return false;
    const entry = createEntry(draft);
    return get().save({
      ...data,
      entries: [entry, ...data.entries],
      updatedAt: new Date().toISOString(),
    });
  },

  async updateEntry(id, draft) {
    const data = get().data;
    if (!data) return { ok: false };
    const current = data.entries.find((entry) => entry.id === id);
    if (!current) return { ok: false };

    const result = applyEntryUpdate(current, draft);
    if (result.blockedReason) {
      return { ok: false, blockedReason: result.blockedReason };
    }

    const saved = await get().save({
      ...data,
      entries: data.entries.map((entry) => (entry.id === id ? result.entry : entry)),
      updatedAt: result.entry.updatedAt,
    });
    return { ok: saved };
  },

  async deleteEntry(id) {
    const data = get().data;
    if (!data) return false;
    return get().save({
      ...data,
      entries: removeEntry(data.entries, id),
      updatedAt: new Date().toISOString(),
    });
  },

  async removeDraft(id) {
    const data = get().data;
    if (!data) return false;
    return get().save(removeDraftFromData(data, id));
  },

  async lock() {
    const sessionId = get().sessionId;
    if (sessionId) {
      await vaultApi.lock(sessionId).catch(() => undefined);
    }
    set({
      status: get().vaultPath ? "locked" : "welcome",
      sessionId: null,
      data: null,
      busy: false,
      error: null,
    });
  },

  async reset() {
    await get().lock();
    set({
      status: "welcome",
      vaultPath: null,
      publicInfo: null,
      sessionId: null,
      data: null,
      error: null,
    });
  },

  clearError() {
    set({ error: null });
  },
}));
