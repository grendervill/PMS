import { Settings2, ShieldCheck, Wand2 } from "lucide-react";
import { KeyRound } from "lucide-react";
import { useState } from "react";
import { EmptyState } from "@/components/ui/EmptyState";
import { AuditView } from "@/features/audit/AuditView";
import { EntriesView } from "@/features/entries/EntriesView";
import { GeneratorView } from "@/features/generator/GeneratorView";
import { VaultSidebar } from "@/features/vault/VaultSidebar";
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
  const data = useVaultStore((state) => state.data);
  const lock = useVaultStore((state) => state.lock);
  useAutoLock({ enabled: true, onLock: () => void lock() });

  const meta = sectionMeta[active];
  const empty = sectionEmpty[active];

  const navigate = (section: AppSection) => {
    setActive(section);
    if (section !== "entries") setEntryToOpen(null);
  };

  const openEntry = (id: string) => {
    setEntryToOpen(id);
    setActive("entries");
  };

  return (
    <div className="flex min-h-screen bg-[var(--bg)] text-[var(--text)]">
      <VaultSidebar
        active={active}
        onNavigate={navigate}
        vaultName={data?.name}
        status="unlocked"
        onLock={() => void lock()}
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

        {active === "entries" ? (
          <EntriesView key={entryToOpen ?? "entries"} initialEntryId={entryToOpen} />
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
