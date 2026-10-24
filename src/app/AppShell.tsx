import {
  ArrowLeftRight,
  CreditCard,
  FileWarning,
  IdCard,
  Lock,
  KeySquare,
  MoreHorizontal,
  Settings2,
  ShieldCheck,
  Timer,
  Wand2,
} from "lucide-react";
import { KeyRound } from "lucide-react";
import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import type { PluginListener } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Modal } from "@/components/ui/Modal";
import { useToast } from "@/components/ui/toast-context";
import { AuditView } from "@/features/audit/AuditView";
import { CardsView } from "@/features/cards/CardsView";
import { DocumentsView } from "@/features/documents/DocumentsView";
import { EntriesView } from "@/features/entries/EntriesView";
import { useEditorDraftStore } from "@/features/entries/draftRegistry";
import type { EntryDraft } from "@/features/entries/entries";
import { GeneratorView } from "@/features/generator/GeneratorView";
import { SettingsView } from "@/features/settings/SettingsView";
import { SSHView } from "@/features/ssh/SSHView";
import { TOTPView } from "@/features/totp/TOTPView";
import { TransferView } from "@/features/transfer/TransferView";
import { VaultSidebar } from "@/features/vault/VaultSidebar";
import { saveDraftBeforeLock } from "@/features/vault/lockFlow";
import { navItems, type AppSection } from "@/features/vault/navigation";
import { useVaultStore } from "@/features/vault/store";
import { useAutoLock } from "@/features/vault/useAutoLock";
import {
  autoLockTimeoutMs,
  resolveAutoLockOption,
} from "@/features/vault/autoLock";
import { cn } from "@/lib/cn";
import { isAndroidPlatform } from "@/lib/platform";

const sectionMeta: Record<AppSection, { title: string; subtitle: string }> = {
  entries: { title: "Записи", subtitle: "Логины, пароли и всё, что нужно помнить" },
  cards: { title: "Карты", subtitle: "Банковские карты, срок действия и CVV" },
  documents: { title: "Документы", subtitle: "Паспорта, СНИЛС и водительские права" },
  totp: { title: "TOTP", subtitle: "Живые коды двухфакторной аутентификации" },
  ssh: { title: "SSH", subtitle: "Ключи для серверов и Git" },
  transfer: { title: "Обмен", subtitle: "Передача хранилища между устройствами PMS" },
  generator: { title: "Генератор", subtitle: "Пароли и логины с настраиваемыми правилами" },
  audit: { title: "Проверка", subtitle: "Слабые и повторяющиеся пароли" },
  settings: { title: "Настройки", subtitle: "Хранилище, безопасность и оформление" },
};

const mobilePrimarySections = new Set<AppSection>(["entries", "totp", "ssh", "settings"]);
const mobilePrimaryItems = navItems.filter((item) => mobilePrimarySections.has(item.id));
const mobileMoreItems = navItems.filter((item) => !mobilePrimarySections.has(item.id));

const sectionEmpty = {
  entries: {
    icon: KeyRound,
    title: "Здесь появятся записи",
    description: "Создайте хранилище, и первая карточка станет началом вашего сейфа.",
  },
  cards: {
    icon: CreditCard,
    title: "Пока нет карт",
    description: "Добавьте банковскую карту, чтобы держать номер, срок и CVV под рукой.",
  },
  documents: {
    icon: IdCard,
    title: "Пока нет документов",
    description: "Паспорт, СНИЛС или водительские права — с оригиналами во вложениях.",
  },
  totp: {
    icon: Timer,
    title: "TOTP пока нет",
    description: "Добавьте TOTP-секрет в запись — коды появятся здесь.",
  },
  ssh: {
    icon: KeySquare,
    title: "SSH-ключей пока нет",
    description: "Создайте ключ для сервера или Git и привяжите его к записи.",
  },
  transfer: {
    icon: ArrowLeftRight,
    title: "Обмен рядом",
    description: "Передайте полную копию хранилища на другое устройство PMS.",
  },
  generator: {
    icon: Wand2,
    title: "Генератор под рукой",
    description:
      "Он откроется после создания хранилища — с длиной, наборами символов и пересозданием.",
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
  const [lockDialogOpen, setLockDialogOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [sshKeyToOpen, setSshKeyToOpen] = useState<string | null>(null);
  const data = useVaultStore((state) => state.data);
  const lock = useVaultStore((state) => state.lock);
  const removeDraft = useVaultStore((state) => state.removeDraft);
  const toast = useToast();

  const saveDraftAndLock = async (): Promise<boolean> => {
    const ok = await saveDraftBeforeLock({
      snapshot: useEditorDraftStore.getState().snapshot
        ? { ...useEditorDraftStore.getState().snapshot!, kind: "entry" }
        : null,
      getData: () => useVaultStore.getState().data,
      save: (nextData) => useVaultStore.getState().save(nextData),
      lock,
    });
    if (!ok) {
      toast.show({
        title: "Не удалось сохранить черновик",
        description: "Хранилище осталось открытым. Проверьте доступ к папке.",
        tone: "error",
      });
    }
    return ok;
  };

  const requestLock = () => {
    if (useEditorDraftStore.getState().snapshot) {
      setLockDialogOpen(true);
      return;
    }
    void saveDraftAndLock();
  };

  const autoLockOption = data ? resolveAutoLockOption(data.settings) : "5m";
  const autoLockMs = autoLockTimeoutMs(autoLockOption);
  useAutoLock({
    enabled: autoLockMs !== null,
    timeoutMs: autoLockMs ?? 5 * 60 * 1000,
    onLock: () => void saveDraftAndLock(),
  });

  useEffect(() => {
    if (!isAndroidPlatform()) return;
    let listener: PluginListener | null = null;
    let mounted = true;
    void onBackButtonPress(() => {
      if (moreOpen) {
        setMoreOpen(false);
        return;
      }
      if (lockDialogOpen) {
        setLockDialogOpen(false);
        return;
      }
      const handled = new CustomEvent("pms:android-back", { cancelable: true });
      const wasHandled = !window.dispatchEvent(handled);
      if (wasHandled) return;
      if (active !== "entries") {
        setActive("entries");
        setEntryToOpen(null);
        return;
      }
      void exit(0);
    })
      .then((registered) => {
        if (mounted) listener = registered;
        else void registered.unregister();
      })
      .catch(() => undefined);
    return () => {
      mounted = false;
      void listener?.unregister();
    };
  }, [moreOpen, lockDialogOpen, active]);

  const meta = sectionMeta[active];
  const empty = sectionEmpty[active];
  const drafts = data?.drafts ?? [];
  const pendingDraft = drafts[0] ?? null;
  const pendingPayload = pendingDraft?.payload as Partial<EntryDraft> | undefined;

  const navigate = (section: AppSection) => {
    setActive(section);
    setSshKeyToOpen(null);
    if (section !== "entries") setEntryToOpen(null);
  };

  const openEntry = (id: string) => {
    setEntryToOpen(id);
    setSshKeyToOpen(null);
    setActive("entries");
  };

  const openSshKey = (id: string) => {
    setSshKeyToOpen(id);
    setEntryToOpen(null);
    setActive("ssh");
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
        onLock={requestLock}
      />

      <main className="flex h-screen min-w-0 flex-1 flex-col pb-[calc(5rem+env(safe-area-inset-bottom))] lg:pb-0">
        <header className="shrink-0 border-b border-[var(--border)] bg-[var(--bg)]/90 px-5 pb-4 pt-10 sm:pt-4 backdrop-blur-md sm:px-7">
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <h1 className="pms-display text-[24px] leading-tight tracking-tight">{meta.title}</h1>
              <p className="mt-0.5 truncate text-[12.5px] text-[var(--muted)]">{meta.subtitle}</p>
            </div>
            <div className="flex items-center gap-2">
              <span className="hidden text-[10.5px] text-[var(--muted)] sm:block">
                {data?.name ?? "Хранилище"}
              </span>
              <button
                type="button"
                aria-label="Заблокировать"
                onClick={requestLock}
                className="flex size-9 items-center justify-center rounded-[var(--radius-sm)] border border-[var(--border)] text-[var(--muted)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--text)] lg:hidden"
              >
                <Lock size={15} />
              </button>
            </div>
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
              draftToRestore ? `${draftToRestore.id}-${restoreVersion}` : (entryToOpen ?? "entries")
            }
            initialEntryId={entryToOpen}
            initialDraft={draftToRestore}
            onRequestLock={requestLock}
            onOpenSshKey={openSshKey}
          />
        ) : active === "cards" ? (
          <CardsView />
        ) : active === "documents" ? (
          <DocumentsView />
        ) : active === "generator" ? (
          <GeneratorView />
        ) : active === "totp" ? (
          <TOTPView onOpenEntry={openEntry} />
        ) : active === "ssh" ? (
          <SSHView
            key={sshKeyToOpen ?? "ssh"}
            initialKeyId={sshKeyToOpen}
            onOpenEntry={openEntry}
          />
        ) : active === "transfer" ? (
          <TransferView />
        ) : active === "audit" ? (
          <AuditView onOpenEntry={openEntry} />
        ) : active === "settings" ? (
          <SettingsView />
        ) : (
          <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
            <div key={active} className="pms-rise mx-auto w-full max-w-5xl">
              <EmptyState icon={empty.icon} title={empty.title} description={empty.description} />
            </div>
          </div>
        )}
      </main>

      <nav
        aria-label="Основная навигация"
        className="fixed inset-x-0 bottom-0 z-30 flex border-t border-[var(--border)] bg-[var(--surface)]/95 px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] pt-1.5 backdrop-blur-md lg:hidden"
      >
        {mobilePrimaryItems.map((item) => {
          const Icon = item.icon;
          const isActive = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              onClick={() => navigate(item.id)}
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
        <button
          type="button"
          aria-current={!mobilePrimarySections.has(active) ? "page" : undefined}
          onClick={() => setMoreOpen(true)}
          className={cn(
            "flex min-w-0 flex-1 flex-col items-center gap-0.5 rounded-[var(--radius-sm)] px-1 py-1.5 text-[10px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
            !mobilePrimarySections.has(active) ? "text-[var(--accent-soft)]" : "text-[var(--muted)]",
          )}
        >
          <MoreHorizontal size={19} strokeWidth={1.8} />
          <span className="truncate">Ещё</span>
        </button>
      </nav>

      <Modal
        open={moreOpen}
        onOpenChange={setMoreOpen}
        title="Ещё"
        description="Дополнительные разделы PMS"
        className="w-[min(92vw,420px)]"
      >
        <div className="grid gap-2">
          {mobileMoreItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => {
                  navigate(item.id);
                  setMoreOpen(false);
                }}
                className="flex h-11 items-center gap-3 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/50 px-3 text-left text-[13px] text-[var(--text-soft)] transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--surface-2)]"
              >
                <Icon size={17} className="text-[var(--accent-soft)]" />
                {item.label}
              </button>
            );
          })}
        </div>
      </Modal>

      <Modal
        open={lockDialogOpen}
        onOpenChange={setLockDialogOpen}
        title="Заблокировать хранилище?"
        description="В форме есть несохранённые изменения."
        className="w-[min(92vw,560px)]"
        footer={
          <>
            <Button variant="ghost" onClick={() => setLockDialogOpen(false)}>
              Отмена
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                void (async () => {
                  useEditorDraftStore.getState().clear();
                  await lock();
                  setLockDialogOpen(false);
                })();
              }}
            >
              Без сохранения
            </Button>
            <Button
              onClick={() => {
                void (async () => {
                  const ok = await saveDraftAndLock();
                  if (ok) setLockDialogOpen(false);
                })();
              }}
            >
              Сохранить черновик
            </Button>
          </>
        }
      >
        <p className="text-[12px] leading-relaxed text-[var(--muted)]">
          Черновик будет сохранён в зашифрованном хранилище и предложен после следующего входа. Если
          заблокировать без сохранения, введённые данные пропадут.
        </p>
      </Modal>
    </div>
  );
}
