import { useEffect, useState } from "react";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import { getSelectedAppIcon } from "@/features/settings/appIcon";
import { ToastProvider } from "@/components/ui/Toast";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { AppShell } from "@/app/AppShell";
import { useVaultStore } from "@/features/vault/store";
import { WelcomeScreen } from "@/features/vault/WelcomeScreen";
import { UnlockScreen } from "@/features/vault/UnlockScreen";
import { StorageModeScreen } from "@/features/storage/StorageModeScreen";
import { ServerProfileForm } from "@/features/storage/ServerProfileForm";
import { saveProfile, setActiveProfile, type ServerProfile } from "@/features/storage/profiles";
import { startupTarget } from "@/features/storage/startup";
import { clearMode, remoteStorage, setMode } from "@/lib/storage";

type Flow = "boot" | "choose" | "server" | "ready";

export default function App() {
  const status = useVaultStore((state) => state.status);
  const [flow, setFlow] = useState<Flow>("boot");
  const [bootError, setBootError] = useState<string | null>(null);

  useEffect(() => {
    void boot();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function boot() {
    const target = startupTarget();

    if (target.kind === "choose") {
      setFlow("choose");
      return;
    }

    if (target.kind === "local") {
      await useVaultStore.getState().init();
      setFlow("ready");
      return;
    }

    try {
      const cacheDir = await vaultApi.remoteCacheDir(target.profile.id);
      // Пробуем подтянуть свежую версию, но без сети работаем с кэшем.
      try {
        const meta = await vaultApi.remoteVaultMeta(target.profile.accountId);
        if (meta && !(await remoteStorage.vaultExists(cacheDir))) {
          await vaultApi.remotePullVault(target.profile.accountId, cacheDir);
        }
      } catch {
        // Офлайн — не ошибка запуска.
      }
      await useVaultStore.getState().initFromRemote(cacheDir);
      setFlow("ready");
    } catch (error) {
      setBootError(errorText(error));
      setFlow("choose");
    }
  }

  useEffect(() => {
    void vaultApi.setAppIcon(getSelectedAppIcon()).catch(() => undefined);
  }, []);

  const chooseLocal = async () => {
    setMode("local");
    setFlow("boot");
    await boot();
  };

  const connected = async (profile: ServerProfile, cacheDir: string) => {
    saveProfile(profile);
    setActiveProfile(profile.id);
    setMode("remote");
    try {
      const meta = await vaultApi.remoteVaultMeta(profile.accountId);
      if (meta) await vaultApi.remotePullVault(profile.accountId, cacheDir);
    } catch {
      // Первый вход без сейфа или без сети: кэш пуст, это не ошибка.
    }
    await useVaultStore.getState().initFromRemote(cacheDir);
    setFlow("ready");
  };

  const changeStorage = () => {
    clearMode();
    setActiveProfile(null);
    useVaultStore.setState({ status: "welcome", initialized: false });
    setFlow("choose");
  };

  return (
    <ToastProvider>
      <ConfirmProvider>
        {flow === "boot" ? (
          <main className="min-h-screen bg-[var(--bg)]" aria-busy="true" />
        ) : flow === "choose" ? (
          <>
            <StorageModeScreen onLocal={() => void chooseLocal()} onServer={() => setFlow("server")} />
            {bootError ? (
              <p
                role="alert"
                className="fixed inset-x-6 bottom-6 mx-auto max-w-lg rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2.5 text-[12px] text-[var(--danger)]"
              >
                {bootError}
              </p>
            ) : null}
          </>
        ) : flow === "server" ? (
          <ServerProfileForm
            onConnected={(profile, cacheDir) => void connected(profile, cacheDir)}
            onCancel={() => setFlow("choose")}
          />
        ) : status === "unlocked" ? (
          <AppShell />
        ) : status === "welcome" ? (
          <WelcomeScreen onChangeStorage={changeStorage} />
        ) : (
          <UnlockScreen />
        )}
      </ConfirmProvider>
    </ToastProvider>
  );
}
