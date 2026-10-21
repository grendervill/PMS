import { Check, Fingerprint, KeyRound, Loader2, Palette, ShieldCheck, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/Button";
import { Toggle } from "@/components/ui/Toggle";
import { useToast } from "@/components/ui/toast-context";
import { ImportWizard } from "@/features/import/ImportWizard";
import {
  APP_ICONS,
  getSelectedAppIcon,
  setSelectedAppIcon,
  type AppIconId,
} from "@/features/settings/appIcon";
import { vaultApi } from "@/features/vault/api";
import { isBiometricEnabled } from "@/features/vault/biometric";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import { getStorage } from "@/lib/storage";
import { cn } from "@/lib/cn";

export function SettingsView() {
  const data = useVaultStore((state) => state.data);
  const busy = useVaultStore((state) => state.busy);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const enrollBiometric = useVaultStore((state) => state.enrollBiometric);
  const disableBiometric = useVaultStore((state) => state.disableBiometric);
  const openVault = useVaultStore((state) => state.openVault);
  const lock = useVaultStore((state) => state.lock);
  const toast = useToast();
  const [importOpen, setImportOpen] = useState(false);
  const [archiveBusy, setArchiveBusy] = useState(false);
  const [selectedIcon, setSelectedIcon] = useState<AppIconId>(() => getSelectedAppIcon());
  const [iconBusy, setIconBusy] = useState<AppIconId | null>(null);
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

  const chooseIcon = async (icon: AppIconId) => {
    setIconBusy(icon);
    try {
      await vaultApi.setAppIcon(icon);
      setSelectedIcon(icon);
      setSelectedAppIcon(icon);
      toast.show({ title: "Значок приложения обновлён", tone: "success" });
    } catch (error) {
      toast.show({
        title: "Не удалось изменить значок",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setIconBusy(null);
    }
  };

  const exportArchive = async () => {
    if (!vaultPath || !data) return;
    setArchiveBusy(true);
    try {
      const destination = await save({
        defaultPath: `${data.name.replace(/[^a-zA-Z0-9._-]+/g, "-") || "pms"}.pmsvault`,
      });
      if (!destination) return;
      await vaultApi.pmsExport(vaultPath, destination);
      toast.show({ title: "Резервная копия создана", description: destination, tone: "success" });
    } catch (error) {
      toast.show({
        title: "Не удалось экспортировать хранилище",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setArchiveBusy(false);
    }
  };

  const importArchive = async () => {
    setArchiveBusy(true);
    const storage = getStorage();
    try {
      const archive = await storage.pickFile("Выберите .pmsvault");
      if (!archive) return;
      const destination = await storage.pickFolder();
      if (!destination) return;
      const imported = await vaultApi.pmsImport(archive, destination);
      await lock();
      await openVault(imported);
    } catch (error) {
      toast.show({
        title: "Не удалось импортировать хранилище",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setArchiveBusy(false);
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
            <Palette size={15} className="text-[var(--accent-soft)]" />
            <h2 className="pms-display text-lg">Выбрать значок приложения</h2>
          </div>
          <div className="px-4 py-4">
            <div className="grid grid-cols-3 gap-2.5">
              {APP_ICONS.map((option) => {
                const active = selectedIcon === option.id;
                return (
                  <button
                    key={option.id}
                    type="button"
                    aria-pressed={active}
                    aria-label={option.label}
                    disabled={iconBusy !== null}
                    onClick={() => void chooseIcon(option.id)}
                    className={cn(
                      "group relative flex min-w-0 flex-col items-center gap-2 rounded-[var(--radius-md)] border px-2 py-2.5 text-center transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
                      active
                        ? "border-[var(--accent)]/60 bg-[var(--accent)]/8"
                        : "border-[var(--border)] bg-[var(--surface-2)]/40 hover:border-[var(--border-strong)] hover:bg-[var(--surface-2)]",
                    )}
                  >
                    <span className="relative">
                      <img
                        src={option.image}
                        alt=""
                        className="size-14 rounded-[14px] object-cover shadow-[0_4px_14px_rgba(0,0,0,0.25)]"
                      />
                      {active ? (
                        <span className="absolute -right-1.5 -top-1.5 grid size-5 place-items-center rounded-full border-2 border-[var(--surface)] bg-[var(--accent)] text-[#08110d]">
                          <Check size={11} strokeWidth={3} />
                        </span>
                      ) : null}
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-[11px] font-medium text-[var(--text-soft)]">
                        {option.label}
                      </span>
                      <span className="mt-0.5 hidden text-[9.5px] text-[var(--muted)] sm:block">
                        {option.description}
                      </span>
                    </span>
                    {iconBusy === option.id ? (
                      <Loader2 size={14} className="absolute right-2 top-2 animate-spin text-[var(--accent-soft)]" />
                    ) : null}
                  </button>
                );
              })}
            </div>
            <p className="mt-3 text-[10.5px] leading-relaxed text-[var(--muted)]">
              Значок в Dock меняется сразу. После перезапуска приложение восстановит
              выбранный вариант.
            </p>
          </div>
        </section>

        <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70">
          <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-3">
            <KeyRound size={15} className="text-[var(--accent-soft)]" />
            <h2 className="pms-display text-lg">Импорт и экспорт</h2>
          </div>
          <div className="grid gap-3 px-4 py-4">
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" onClick={() => setImportOpen(true)}>
                <Upload size={15} />
                Импорт из менеджера
              </Button>
              <Button
                variant="secondary"
                disabled={archiveBusy}
                onClick={() => void exportArchive()}
              >
                {archiveBusy ? <Loader2 size={15} className="animate-spin" /> : <Upload size={15} />}
                Экспорт .pmsvault
              </Button>
              <Button
                variant="secondary"
                disabled={archiveBusy}
                onClick={() => void importArchive()}
              >
                <Upload size={15} />
                Импорт .pmsvault
              </Button>
            </div>
            <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
              Резервная копия содержит зашифрованное хранилище и вложения. Импорт
              `.pmsvault` создаёт отдельное хранилище в пустой папке.
            </p>
          </div>
        </section>
      </div>

      {importOpen ? <ImportWizard onClose={() => setImportOpen(false)} /> : null}
    </div>
  );
}
