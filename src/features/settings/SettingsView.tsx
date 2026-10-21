import { Archive, Check, Fingerprint, KeyRound, Loader2, Palette, ShieldCheck, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Toggle } from "@/components/ui/Toggle";
import { useToast } from "@/components/ui/toast-context";
import { ImportWizard } from "@/features/import/ImportWizard";
import {
  cleanupBackups,
  createManualBackup,
  lastBackupAt,
} from "@/features/vault/backups";
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
import { exportVaultArchive, importVaultArchive } from "@/lib/storage/archive";
import { isAndroidPlatform } from "@/lib/platform";
import { cn } from "@/lib/cn";

export function SettingsView() {
  const data = useVaultStore((state) => state.data);
  const busy = useVaultStore((state) => state.busy);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const enrollBiometric = useVaultStore((state) => state.enrollBiometric);
  const disableBiometric = useVaultStore((state) => state.disableBiometric);
  const openVault = useVaultStore((state) => state.openVault);
  const saveVault = useVaultStore((state) => state.save);
  const lock = useVaultStore((state) => state.lock);
  const toast = useToast();
  const [importOpen, setImportOpen] = useState(false);
  const [archiveBusy, setArchiveBusy] = useState(false);
  const [selectedIcon, setSelectedIcon] = useState<AppIconId>(() => getSelectedAppIcon());
  const [iconBusy, setIconBusy] = useState<AppIconId | null>(null);
  const android = isAndroidPlatform();
  const [backupBusy, setBackupBusy] = useState(false);
  const [lastBackup, setLastBackup] = useState<Date | null>(null);
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

  useEffect(() => {
    if (!vaultPath) return;
    let active = true;
    void lastBackupAt(vaultPath)
      .then((date) => {
        if (active) setLastBackup(date);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [vaultPath]);

  const updateBackupSetting = async (
    key: "backupRetentionDays" | "autoBackupDays",
    value: number,
  ) => {
    if (!data) return;
    setBackupBusy(true);
    try {
      const saved = await saveVault({
        ...data,
        settings: { ...data.settings, [key]: value },
      });
      if (!saved) {
        toast.show({
          title: "Не удалось сохранить настройку",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
        return;
      }
      if (vaultPath) {
        await cleanupBackups(
          vaultPath,
          key === "backupRetentionDays" ? value : data.settings.backupRetentionDays,
        );
      }
      setLastBackup(vaultPath ? await lastBackupAt(vaultPath) : null);
    } finally {
      setBackupBusy(false);
    }
  };

  const backupNow = async () => {
    if (!data || !vaultPath) return;
    setBackupBusy(true);
    try {
      const created = await createManualBackup(vaultPath, data.settings);
      setLastBackup(await lastBackupAt(vaultPath));
      toast.show({
        title: created ? "Резервная копия создана" : "Резервная копия уже свежая",
        tone: "success",
      });
    } catch (error) {
      toast.show({
        title: "Не удалось создать резервную копию",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setBackupBusy(false);
    }
  };

  const changeBiometric = async (next: boolean) => {
    if (next) {
      const ok = await enrollBiometric();
      if (ok) {
        setEnabled(true);
        toast.show({
          title: "Биометрия включена",
          description: android
            ? "При запуске приложение сразу предложит биометрию."
            : "При запуске приложение сразу предложит Touch ID.",
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
      const destination = await exportVaultArchive(
        vaultPath,
        `${data.name.replace(/[^a-zA-Z0-9._-]+/g, "-") || "pms"}.pmsvault`,
      );
      if (!destination) return;
      toast.show({
        title: "Резервная копия создана",
        description: isAndroidPlatform()
          ? getStorage().displayName(destination)
          : destination,
        tone: "success",
      });
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
      const imported = await importVaultArchive(archive, destination);
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
                  ? "Проверяем доступность биометрии…"
                  : available === true
                    ? "При запуске приложение сразу попросит отпечаток. Отмена или 5 неудач вернут ввод пароль-фразы."
                    : android
                      ? "На этом устройстве нет доступной биометрии. Вход по пароль-фразе или ключ-файлу остаётся."
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
              {android
                ? "Значок на главном экране меняется сразу. Если система кэширует старый значок, перезапустите launcher."
                : "Значок в Dock меняется сразу. После перезапуска приложение восстановит выбранный вариант."}
            </p>
          </div>
        </section>

        <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70">
          <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-3">
            <Archive size={15} className="text-[var(--accent-soft)]" />
            <h2 className="pms-display text-lg">Резервные копии</h2>
          </div>
          <div className="grid gap-3 px-4 py-4">
            <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
              Бэкапы хранятся в папке <span className="pms-mono">backups/</span>. Самый
              свежий не удаляется автоматически никогда.
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="grid gap-1.5 text-[11px] text-[var(--text-soft)]">
                Хранить бэкапы
                <select
                  value={data?.settings.backupRetentionDays ?? 7}
                  onChange={(event) =>
                    void updateBackupSetting(
                      "backupRetentionDays",
                      Number(event.target.value),
                    )
                  }
                  disabled={backupBusy}
                  className="h-9 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-2.5 text-[12px] text-[var(--text)] outline-none focus:border-[var(--accent)]"
                >
                  <option value={1}>1 день</option>
                  <option value={7}>7 дней</option>
                  <option value={30}>30 дней</option>
                </select>
              </label>
              <label className="grid gap-1.5 text-[11px] text-[var(--text-soft)]">
                Автобекап
                <select
                  value={data?.settings.autoBackupDays ?? 1}
                  onChange={(event) =>
                    void updateBackupSetting("autoBackupDays", Number(event.target.value))
                  }
                  disabled={backupBusy}
                  className="h-9 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-2.5 text-[12px] text-[var(--text)] outline-none focus:border-[var(--accent)]"
                >
                  <option value={0}>Выключен</option>
                  <option value={1}>Раз в день</option>
                  <option value={7}>Раз в 7 дней</option>
                  <option value={30}>Раз в 30 дней</option>
                </select>
              </label>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-[10.5px] text-[var(--muted)]">
                Последний:{" "}
                {lastBackup
                  ? lastBackup.toLocaleString("ru-RU", {
                      day: "numeric",
                      month: "short",
                      year: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    })
                  : "ещё нет"}
              </span>
              <Button variant="secondary" size="sm" onClick={() => void backupNow()} disabled={backupBusy}>
                {backupBusy ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Archive size={14} />
                )}
                Создать бэкап сейчас
              </Button>
            </div>
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
