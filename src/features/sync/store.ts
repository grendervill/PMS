import { create } from "zustand";
import { remoteStorage } from "@/lib/storage";
import { vaultApi } from "@/features/vault/api";
import { activeProfile, type ServerProfile } from "@/features/storage/profiles";
import { setVaultSavedListener } from "@/features/vault/store";
import { startSync, type SyncController, type SyncStatus } from "./controller";
import { createSyncEngine } from "./engine";

type SyncStore = {
  status: SyncStatus;
  profile: ServerProfile | null;
  cacheDir: string | null;
  start(profile: ServerProfile, cacheDir: string): void;
  stop(): void;
  markDirty(): void;
  syncNow(): Promise<void>;
  resolveKeepMine(): Promise<void>;
  resolveTakeTheirs(): Promise<void>;
};

let controller: SyncController | null = null;

export const useSyncStore = create<SyncStore>((set, get) => ({
  status: { kind: "idle" },
  profile: null,
  cacheDir: null,

  start(profile, cacheDir) {
    get().stop();

    const engine = createSyncEngine({
      accountId: profile.accountId,
      cacheDir,
      storage: remoteStorage,
      remote: vaultApi,
    });

    controller = startSync({
      accountId: profile.accountId,
      cacheDir,
      storage: remoteStorage,
      engine,
      remote: vaultApi,
      onStatus: (status) => set({ status }),
    });

    setVaultSavedListener(() => get().markDirty());
    set({ profile, cacheDir, status: { kind: "idle" } });
  },

  stop() {
    controller?.stop();
    controller = null;
    setVaultSavedListener(null);
    set({ status: { kind: "idle" }, profile: null, cacheDir: null });
  },

  markDirty() {
    controller?.markDirty();
  },

  async syncNow() {
    await controller?.syncNow();
  },

  async resolveKeepMine() {
    await controller?.resolveKeepMine();
  },

  async resolveTakeTheirs() {
    await controller?.resolveTakeTheirs();
  },
}));

/** Профиль и кэш активной сессии — для экранов, которые не держат стор. */
export function currentSyncTarget(): { profile: ServerProfile; cacheDir: string } | null {
  const state = useSyncStore.getState();
  if (state.profile && state.cacheDir) {
    return { profile: state.profile, cacheDir: state.cacheDir };
  }
  const profile = activeProfile();
  return profile ? { profile, cacheDir: "" } : null;
}
