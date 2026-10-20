import { Settings2, ShieldCheck, Wand2, FileWarning } from "lucide-react";
import { KeyRound } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { useToast } from "@/components/ui/toast-context";
import { AuditView } from "@/features/audit/AuditView";
import { EntriesView } from "@/features/entries/EntriesView";
import { useEditorDraftStore } from "@/features/entries/draftRegistry";
import type { EntryDraft } from "@/features/entries/entries";
import { GeneratorView } from "@/features/generator/GeneratorView";
import { VaultSidebar } from "@/features/vault/VaultSidebar";
import { saveDraft } from "@/features/vault/drafts";
import { navItems, type AppSection } from "@/features/vault/navigation";
import { useVaultStore } from "@/features/vault/store";
import { useAutoLock } from "@/features/vault/useAutoLock";
import { cn } from "@/lib/cn";

const sectionMeta: Record<AppSection, { title: string; subtitle: string }> = {
  entries: { title: "Записи", subtitle: "Логины, пароли и всё, что нужно помнить" },
  generator: { title: "Генератор", subtitle: "Пароли и логины с настраиваемыми правилами" },
  audit: { title: "Проверка", subtitle: "Слабые и повторяющиеся пароли" },
  settings: { title: "Настройки", subtitle: "Хранилище, безопасность и оформление" },
};

const sectionEmpty = {
  entries: {
    icon: KeyRound,
    title: "Здесь появятся записи",
    description: "Создайте хранилище, и первая карточка станет началом вашего сейфа.",
  },
  generator: {
    icon: Wand2,
    title: "Генератор под рукой",
    description: "Он откроется после создания хранилища — с длиной, наборами символов и пересозданием.",
  },
  audit: {
    icon: ShieldCheck,
    title: "Проверка будет здесь",
    description: "Покажем слабые пароли и повторы, не мешая сохранять то, что вы выбрали.",
  },
  settings: {
    icon: Settings2,
    title: "Настройки хранилища",
    description: "Автоблокировка, буфер обмена, резервные копии и внешний вид.",
  },
} satisfies Record<AppSection, { icon: typeof KeyRound; title: string; description: string }>;

export function AppShell() {
  const [active, setActive] = useState<AppSection>("entries");
  const [entryToOpen, setEntryToOpen] = useState<string | null>(null);
  const [draftToRestore, setDraftToRestore] = useState<{
    id: string;
    payload: EntryDraft;
  } | null>(null);
  const [restoreVersion, setRestoreVersion] = useState(0);
  const data = useVaultStore((state) => state.data);
  const lock = useVaultStore((state) => state.lock);
  const removeDraft = useVaultStore((state) => state.removeDraft);
  const toast = useToast();

  const saveDraftAndLock = async () => {
    const snapshot = useEditorDraftStore.getState().snapshot;
    if (snapshot) {
      const current = useVaultStore.getState().data;
      if (current) {
        const next = saveDraft(current, {
          id: snapshot.id,
          kind: "entry",
          payload: snapshot.payload,
        });
        const saved = await useVaultStore.getState().save(next);
        if (!saved) {
          toast.show({
            title: "Не удалось сохранить черновик",
            description: "Хранилище осталось открытым. Проверьте доступ к папке.",
            tone: "error",
          });
          return;
        }
      }
    }
    await lock();
  };

  useAutoLock({ enabled: true, onLock: () => void saveDraftAndLock() });

  const meta = sectionMeta[active];
  const empty = sectionEmpty[active];
  const drafts = data?.drafts ?? [];
  const pendingDraft = drafts[0] ?? null;
  const pendingPayload = pendingDraft?.payload as Partial<EntryDraft> | undefined;

  const navigate = (section: AppSection) => {
    setActive(section);
    if (section !== "entries") setEntryToOpen(null);
  };

  const openEntry = (id: string) => {
    setEntryToOpen(id);
    setActive("entries");
  };

  const restoreDraft = (id: string, payload: EntryDraft) => {
    setDraftToRestore({ id, payload });
    setRestoreVersion((current) => current + 1);
    setEntryToOpen(null);
    setActive("entries");
  };

  return (
    <div className="flex min-h-screen bg-[var(--bg)] text-[var(--text)]">
      <VaultSidebar
        active={active}
        onNavigate={navigate}
        vaultName={data?.name}
        status="unlocked"
        onLock={() => void saveDraftAndLock()}
      />

      <main className="flex h-screen min-w-0 flex-1 flex-col pb-20 lg:pb-0">
        <header className="shrink-0 border-b border-[var(--border)] bg-[var(--bg)]/90 px-5 py-4 backdrop-blur-md sm:px-7">
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <h1 className="pms-display text-[24px] leading-tight tracking-tight">{meta.title}</h1>
              <p className="mt-0.5 truncate text-[12.5px] text-[var(--muted)]">{meta.subtitle}</p>
            </div>
            <span className="hidden text-[10.5px] text-[var(--muted)] sm:block">
              {data?.name ?? "Хранилище"}
            </span>
          </div>
        </header>

        {pendingDraft ? (
          <div className="shrink-0 border-b border-[var(--warning)]/25 bg-[var(--warning)]/5 px-5 py-2.5 sm:px-7">
            <div className="mx-auto flex max-w-5xl items-center gap-3">
              <FileWarning size={15} className="shrink-0 text-[var(--warning)]" />
              <p className="min-w-0 flex-1 truncate text-[11.5px] text-[var(--text-soft)]">
                Несохранённый черновик
                {pendingPayload?.name ? `: ${pendingPayload.name}` : " записи"}
              </p>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => restoreDraft(pendingDraft.id, pendingDraft.payload as EntryDraft)}
              >
                Восстановить
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  void removeDraft(pendingDraft.id);
                  if (draftToRestore?.id === pendingDraft.id) setDraftToRestore(null);
                }}
              >
                Удалить
              </Button>
            </div>
          </div>
        ) : null}

        {active === "entries" ? (
          <EntriesView
            key={
              draftToRestore
                ? `${draftToRestore.id}-${restoreVersion}`
                : (entryToOpen ?? "entries")
            }
            initialEntryId={entryToOpen}
            initialDraft={draftToRestore}
          />
        ) : active === "generator" ? (
          <GeneratorView />
        ) : active === "audit" ? (
          <AuditView onOpenEntry={openEntry} />
        ) : (
          <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
            <div key={active} className="pms-rise mx-auto w-full max-w-5xl">
              <EmptyState
                icon={empty.icon}
                title={empty.title}
                description={empty.description}
              />
            </div>
          </div>
        )}
      </main>

      <nav
        aria-label="Основная навигация"
        className="fixed inset-x-0 bottom-0 z-30 flex border-t border-[var(--border)] bg-[var(--surface)]/95 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] pt-1.5 backdrop-blur-md lg:hidden"
      >
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              onClick={() => setActive(item.id)}
              className={cn(
                "flex min-w-0 flex-1 flex-col items-center gap-0.5 rounded-[var(--radius-sm)] px-1 py-1.5 text-[10px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
                isActive ? "text-[var(--accent-soft)]" : "text-[var(--muted)]",
              )}
            >
              <Icon size={19} strokeWidth={1.8} />
              <span className="truncate">{item.label}</span>
            </button>
          );
        })}
      </nav>
    </div>
  );
}
