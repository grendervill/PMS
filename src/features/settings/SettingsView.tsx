import { Fingerprint, KeyRound, Loader2, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { Toggle } from "@/components/ui/Toggle";
import { useToast } from "@/components/ui/toast-context";
import { vaultApi } from "@/features/vault/api";
import { isBiometricEnabled } from "@/features/vault/biometric";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";

export function SettingsView() {
  const data = useVaultStore((state) => state.data);
  const busy = useVaultStore((state) => state.busy);
  const enrollBiometric = useVaultStore((state) => state.enrollBiometric);
  const disableBiometric = useVaultStore((state) => state.disableBiometric);
  const toast = useToast();
  const [available, setAvailable] = useState<boolean | null>(null);
  const [checking, setChecking] = useState(true);
  const [enabled, setEnabled] = useState(() => isBiometricEnabled(data?.id));

  useEffect(() => {
    let active = true;
    void vaultApi
      .biometricStatus()
      .then((status) => {
        if (active) setAvailable(status.available);
      })
      .catch(() => {
        if (active) setAvailable(false);
      })
      .finally(() => {
        if (active) setChecking(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const changeBiometric = async (next: boolean) => {
    if (next) {
      const ok = await enrollBiometric();
      if (ok) {
        setEnabled(true);
        toast.show({
          title: "Биометрия включена",
          description: "При запуске приложение сразу предложит Touch ID.",
          tone: "success",
        });
      } else {
        toast.show({
          title: "Не удалось включить биометрию",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
      }
    } else {
      const ok = await disableBiometric();
      if (ok) {
        setEnabled(false);
        toast.show({ title: "Биометрия выключена", tone: "success" });
      } else {
        toast.show({
          title: "Не удалось выключить биометрию",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
      }
    }
  };

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto grid w-full max-w-3xl gap-4">
        <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70">
          <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-3">
            <Fingerprint size={15} className="text-[var(--accent-soft)]" />
            <h2 className="pms-display text-lg">Безопасность</h2>
          </div>
          <div className="grid gap-3 px-4 py-4">
            <Toggle
              checked={enabled}
              disabled={busy || checking || available !== true}
              onCheckedChange={(next) => void changeBiometric(next)}
              label="Всегда разблокировать при помощи биометрии"
              description={
                checking
                  ? "Проверяем доступность Touch ID…"
                  : available === true
                    ? "При запуске приложение сразу попросит отпечаток. Отмена или 5 неудач вернут ввод пароль-фразы."
                    : "На этом Mac нет доступного Touch ID. Вход по пароль-фразе или ключ-файлу остаётся."
              }
            />
            {enabled && available === true ? (
              <p className="flex items-center gap-1.5 text-[10.5px] text-[var(--accent-soft)]">
                <ShieldCheck size={12} />
                Ключ хранится в системном Keychain и не переносится вместе с хранилищем.
              </p>
            ) : null}
            {checking ? (
              <span className="flex items-center gap-1.5 text-[10.5px] text-[var(--muted)]">
                <Loader2 size={12} className="animate-spin" />
                Проверяем биометрию
              </span>
            ) : null}
          </div>
        </section>

        <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70">
          <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-3">
            <KeyRound size={15} className="text-[var(--accent-soft)]" />
            <h2 className="pms-display text-lg">Импорт и экспорт</h2>
          </div>
          <p className="px-4 py-4 text-[11.5px] leading-relaxed text-[var(--muted)]">
            Импорт из Bitwarden, 1Password, KeePass, Kaspersky и браузеров, а также
            резервная копия `.pmsvault` появятся в следующем обновлении.
          </p>
        </section>
      </div>
    </div>
  );
}
