import { create } from "zustand";
import { remoteStorage } from "@/lib/storage";
import { vaultApi } from "@/features/vault/api";
import { activeProfile, type ServerProfile } from "@/features/storage/profiles";
import { setVaultSavedListener, setVaultUnlockedListener, useVaultStore } from "@/features/vault/store";
import type { UnlockSecret } from "@/features/vault/types";
import { toRemoteProfile } from "@/features/storage/profiles";
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
  /** Вход на сервер после разблокировки сейфа: сессия живёт только в памяти. */
  reauthenticate(secret: UnlockSecret | null): Promise<boolean>;
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
      onPulled: async () => {
        await useVaultStore.getState().reloadFromDisk();
      },
      onNeedsAuth: async () => {
        const current = get().profile;
        if (!current) return false;
        try {
          // Семя identity-ключа хранится в Rust; мастер-ключ не нужен.
          await vaultApi.remoteRefresh(current.accountId);
          return true;
        } catch {
          return false;
        }
      },
      onStatus: (status) => set({ status }),
    });

    setVaultSavedListener(() => get().markDirty());
    setVaultUnlockedListener((secret) => void get().reauthenticate(secret));
    set({ profile, cacheDir, status: { kind: "idle" } });
  },

  stop() {
    controller?.stop();
    controller = null;
    setVaultSavedListener(null);
    setVaultUnlockedListener(null);
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

  async reauthenticate(secret) {
    const { profile, cacheDir } = get();
    if (!profile || !cacheDir) return false;

    try {
      const remote = toRemoteProfile(profile);
      if (secret) {
        await vaultApi.remoteLogin(remote, secret);
      } else {
        // Биометрия: семя identity-ключа уже в памяти Rust после разблокировки.
        await vaultApi.remoteLoginBiometric(remote);
      }
    } catch {
      set({ status: { kind: "needs-auth" } });
      return false;
    }

    // Сессия появилась — будим синхронизацию, не дожидаясь таймера.
    if (!controller) {
      get().start(profile, cacheDir);
    }
    await controller?.resumeAfterAuth();
    return true;
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
