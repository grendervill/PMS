import { useState } from "react";
import { ChevronRight, FolderOpen, Lock, Plus, ShieldCheck, Upload } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { getStorage } from "@/lib/storage";
import { importVaultArchive } from "@/lib/storage/archive";
import { CreateVaultWizard } from "@/features/vault/CreateVaultWizard";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}

export function WelcomeScreen() {
  const recent = useVaultStore((state) => state.recent);
  const busy = useVaultStore((state) => state.busy);
  const error = useVaultStore((state) => state.error);
  const openVault = useVaultStore((state) => state.openVault);
  const clearError = useVaultStore((state) => state.clearError);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [archiveBusy, setArchiveBusy] = useState(false);
  const [archiveError, setArchiveError] = useState<string | null>(null);

  if (wizardOpen) {
    return <CreateVaultWizard onClose={() => setWizardOpen(false)} />;
  }

  const openExisting = async () => {
    clearError();
    const path = await getStorage().pickFolder();
    if (path) await openVault(path);
  };

  const importArchive = async () => {
    setArchiveBusy(true);
    setArchiveError(null);
    try {
      const storage = getStorage();
      const archive = await storage.pickFile("Выберите .pmsvault");
      if (!archive) return;
      const destination = await storage.pickFolder();
      if (!destination) return;
      const imported = await importVaultArchive(archive, destination);
      await openVault(imported);
    } catch (importError) {
      setArchiveError(errorText(importError));
    } finally {
      setArchiveBusy(false);
    }
  };

  return (
    <main className="relative min-h-screen overflow-hidden bg-[var(--bg)] text-[var(--text)]">
      <div className="mx-auto grid min-h-screen w-full max-w-6xl lg:grid-cols-[1.08fr_0.92fr]">
        <section className="relative hidden flex-col justify-between border-r border-[var(--border)] bg-[var(--surface)]/45 p-10 lg:flex xl:p-14">
          <div className="flex items-center gap-3">
            <div className="grid size-11 place-items-center rounded-[12px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent)]">
              <Lock size={20} strokeWidth={1.7} />
            </div>
            <div>
              <p className="pms-display text-[24px] leading-none tracking-tight">PMS</p>
              <p className="mt-1 text-[11.5px] text-[var(--muted)]">Личное хранилище</p>
            </div>
          </div>

          <div className="max-w-md">
            <p className="mb-5 text-[12px] font-medium text-[var(--seal)]">Ваш сейф, ваши правила</p>
            <h2 className="pms-display text-[44px] leading-[1.04] tracking-tight text-[var(--text)]">
              Пароли, коды и ключи остаются у вас.
            </h2>
            <p className="mt-5 max-w-sm text-[14px] leading-relaxed text-[var(--muted)]">
              Одна запечатанная папка. Без аккаунта, без облака и без чужого сервера.
              Скопируйте её на другое устройство — и хранилище с вами.
            </p>
          </div>

          <div className="flex items-center gap-5 text-[11.5px] text-[var(--muted)]">
            <span className="flex items-center gap-1.5">
              <ShieldCheck size={14} className="text-[var(--accent-soft)]" />
              Зашифровано
            </span>
            <span className="size-1 rounded-full bg-[var(--border-strong)]" />
            <span>Локально</span>
            <span className="size-1 rounded-full bg-[var(--border-strong)]" />
            <span>Переносимо</span>
          </div>

          <div className="pms-seam pointer-events-none absolute inset-y-0 right-0 w-px" />
        </section>

        <section className="flex flex-col justify-center px-6 py-10 sm:px-10 lg:px-12 xl:px-16">
          <div className="mb-9 flex items-center gap-3 lg:hidden">
            <div className="grid size-10 place-items-center rounded-[11px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent)]">
              <Lock size={18} />
            </div>
            <div>
              <p className="pms-display text-[21px] leading-none">PMS</p>
              <p className="mt-1 text-[11px] text-[var(--muted)]">Личное хранилище</p>
            </div>
          </div>

          <div className="w-full max-w-[420px]">
            <h1 className="pms-display text-[34px] leading-tight tracking-tight">
              Добро пожаловать
            </h1>
            <p className="mt-2 max-w-sm text-[13.5px] leading-relaxed text-[var(--muted)]">
              Создайте новое хранилище или откройте папку, которую уже переносили.
            </p>

            <div className="mt-7 flex flex-col gap-2.5">
              <Button size="md" className="h-11 justify-start px-4" onClick={() => setWizardOpen(true)}>
                <Plus size={17} />
                Создать хранилище
              </Button>
              <Button
                size="md"
                variant="secondary"
                className="h-11 justify-start px-4"
                onClick={() => void openExisting()}
                disabled={busy}
              >
                <FolderOpen size={17} />
                Открыть существующее
              </Button>
              <Button
                size="md"
                variant="secondary"
                className="h-11 justify-start px-4"
                onClick={() => void importArchive()}
                disabled={busy || archiveBusy}
              >
                <Upload size={17} />
                Импортировать .pmsvault
              </Button>
            </div>

            {archiveError ? (
              <p
                role="alert"
                className="mt-3 rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2.5 text-[11.5px] leading-relaxed text-[var(--danger)]"
              >
                {archiveError}
              </p>
            ) : null}

            {error ? (
              <div
                role="alert"
                className="mt-4 rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2.5 text-[12px] leading-relaxed text-[var(--danger)]"
              >
                {errorText(error)}
              </div>
            ) : null}

            {recent.length > 0 ? (
              <div className="mt-9">
                <div className="mb-2 flex items-center justify-between">
                  <h2 className="text-[11.5px] font-medium text-[var(--muted)]">Недавние</h2>
                  <span className="text-[10.5px] text-[var(--muted)]/70">{recent.length}</span>
                </div>
                <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/60">
                  {recent.slice(0, 5).map((item) => (
                    <button
                      key={item.path}
                      type="button"
                      disabled={busy}
                      onClick={() => void openVault(item.path)}
                      className="group flex w-full items-center gap-3 border-b border-[var(--border)] px-3.5 py-3 text-left transition-colors last:border-b-0 hover:bg-[var(--surface-2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--accent)] disabled:opacity-50"
                    >
                      <span className="grid size-8 shrink-0 place-items-center rounded-[8px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                        <Lock size={14} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] font-medium text-[var(--text-soft)]">
                          {item.name}
                        </span>
                        <span className="mt-0.5 block truncate text-[10.5px] text-[var(--muted)]">
                          {item.path}
                        </span>
                      </span>
                      <span className="shrink-0 text-[10.5px] text-[var(--muted)]">
                        {formatDate(item.lastOpenedAt)}
                      </span>
                      <ChevronRight
                        size={14}
                        className="shrink-0 text-[var(--muted)] transition-transform group-hover:translate-x-0.5"
                      />
                    </button>
                  ))}
                </div>
              </div>
            ) : null}

            <p className="mt-9 text-[10.5px] leading-relaxed text-[var(--muted)]/75">
              Пароль-фраза не хранится в программе. Если потерять все способы входа,
              восстановить данные будет невозможно.
            </p>
          </div>
        </section>
      </div>
    </main>
  );
}
