import { useCallback, useEffect, useState } from "react";
import { useContext } from "react";
import { ConfirmContext } from "@/components/ui/confirm-context";
import { Modal } from "@/components/ui/Modal";
import { Button } from "@/components/ui/Button";
import { SecretInput } from "@/components/ui/SecretInput";
import { useSyncStore } from "@/features/sync/store";
import { useVaultStore } from "@/features/vault/store";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import type { ServerUsage } from "@/features/vault/types";
import { clearMode, setMode } from "@/lib/storage";
import { removeProfile, setActiveProfile } from "@/features/storage/profiles";
import { ServerSection } from "./ServerSection";

export function ServerSectionContainer() {
  const profile = useSyncStore((state) => state.profile);
  const status = useSyncStore((state) => state.status);
  const syncNow = useSyncStore((state) => state.syncNow);
  const stop = useSyncStore((state) => state.stop);
  const reauthenticate = useSyncStore((state) => state.reauthenticate);

  const [usage, setUsage] = useState<ServerUsage | null>(null);
  const [stale, setStale] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [reauthOpen, setReauthOpen] = useState(false);
  const [reauthSecret, setReauthSecret] = useState("");
  const [reauthError, setReauthError] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirm = useContext(ConfirmContext);

  const refreshUsage = useCallback(async () => {
    if (!profile) return;
    try {
      setUsage(await vaultApi.remoteUsage(profile.accountId));
      setStale(false);
    } catch {
      setStale(true);
    }
  }, [profile]);

  useEffect(() => {
    void refreshUsage();
  }, [refreshUsage, status.kind]);

  if (!profile) return null;

  const disconnect = async () => {
    if (!confirm) return;
    const approved = await confirm({
      title: "Отключиться от сервера?",
      description:
        "Синхронизация остановится, локальная копия останется в кэше. Сохраните её в обычную папку, если хотите ею пользоваться дальше.",
      confirmLabel: "Отключиться",
    });
    if (!approved) return;

    await vaultApi.remoteLogout(profile.accountId).catch(() => undefined);
    stop();
    setActiveProfile(null);
    setMode("local");
    useVaultStore.setState({ status: "welcome", initialized: false });
    window.location.reload();
  };

  const deleteAccount = async () => {
    setBusy(true);
    setError(null);
    try {
      await vaultApi.remoteDeleteAccount(profile.accountId, {
        kind: "passphrase",
        value: passphrase,
      });
      await vaultApi.remoteLogout(profile.accountId).catch(() => undefined);
      stop();
      removeProfile(profile.id);
      setActiveProfile(null);
      clearMode();
      window.location.reload();
    } catch (failure) {
      setError(errorText(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <ServerSection
        profile={profile}
        status={status}
        usage={usage}
        stale={stale}
        onSyncNow={() => void syncNow().then(refreshUsage)}
        onDisconnect={() => void disconnect()}
        onDeleteAccount={() => setDeleteOpen(true)}
        onReauthenticate={() => setReauthOpen(true)}
      />

      <Modal
        open={reauthOpen}
        onOpenChange={setReauthOpen}
        title="Вход на сервер"
        description="Сессия сервера живёт только в памяти приложения. После перезапуска нужен мастер-пароль — тот же, что открывает сейф."
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={() => setReauthOpen(false)}>
              Отмена
            </Button>
            <Button
              onClick={() => {
                setReauthError(null);
                void reauthenticate({ kind: "passphrase", value: reauthSecret }).then((ok) => {
                  if (ok) {
                    setReauthOpen(false);
                    setReauthSecret("");
                  } else {
                    setReauthError("Не удалось войти: проверьте мастер-пароль");
                  }
                });
              }}
              disabled={reauthSecret.length === 0}
            >
              Войти
            </Button>
          </div>
        }
      >
        <div className="grid gap-3">
          <SecretInput
            value={reauthSecret}
            onChange={(event) => setReauthSecret(event.target.value)}
            placeholder="Пароль-фраза"
            aria-label="Пароль-фраза"
          />
          {reauthError ? (
            <p role="alert" className="text-[11.5px] text-[var(--danger)]">
              {reauthError}
            </p>
          ) : null}
        </div>
      </Modal>

      <Modal
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Удалить аккаунт на сервере?"
        description="Сервер сотрёт сейф и вложения. Локальная копия останется в кэше, но синхронизировать её будет некуда."
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={() => setDeleteOpen(false)} disabled={busy}>
              Отмена
            </Button>
            <Button
              variant="danger"
              onClick={() => void deleteAccount()}
              disabled={busy || passphrase.length === 0}
            >
              Удалить навсегда
            </Button>
          </div>
        }
      >
        <div className="grid gap-3">
          <p className="text-[12.5px] leading-relaxed text-[var(--text-soft)]">
            Подтвердите мастер-ключом: сервер принимает удаление только с подписью.
          </p>
          <SecretInput
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
            placeholder="Пароль-фраза"
            aria-label="Пароль-фраза"
          />
          {error ? (
            <p role="alert" className="text-[11.5px] text-[var(--danger)]">
              {error}
            </p>
          ) : null}
        </div>
      </Modal>
    </>
  );
}
