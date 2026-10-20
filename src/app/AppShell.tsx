import { Plus, Search, Settings2, ShieldCheck, Wand2 } from "lucide-react";
import { KeyRound } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
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
  const data = useVaultStore((state) => state.data);
  const lock = useVaultStore((state) => state.lock);
  useAutoLock({ enabled: true, onLock: () => void lock() });

  const meta = sectionMeta[active];
  const empty = sectionEmpty[active];

  return (
    <div className="flex min-h-screen bg-[var(--bg)] text-[var(--text)]">
      <VaultSidebar
        active={active}
        onNavigate={setActive}
        vaultName={data?.name}
        status="unlocked"
        onLock={() => void lock()}
      />

      <main className="flex min-w-0 flex-1 flex-col pb-20 lg:pb-0">
        <header className="sticky top-0 z-20 border-b border-[var(--border)] bg-[var(--bg)]/90 px-5 py-4 backdrop-blur-md sm:px-7">
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <h1 className="pms-display text-[24px] leading-tight tracking-tight">{meta.title}</h1>
              <p className="mt-0.5 truncate text-[12.5px] text-[var(--muted)]">{meta.subtitle}</p>
            </div>

            <div className="flex items-center gap-2">
              <div className="relative hidden w-64 md:block">
                <Search
                  size={15}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--muted)]"
                />
                <Input
                  aria-label="Поиск по записям"
                  placeholder="Поиск"
                  className="h-9 pl-9"
                  disabled
                />
              </div>
              <Button
                size="sm"
                variant="secondary"
                disabled
                title="Станет доступно после создания хранилища"
              >
                <Plus size={15} />
                Новая запись
              </Button>
            </div>
          </div>
        </header>

        <div className="flex-1 px-5 py-6 sm:px-7">
          <div key={active} className="pms-rise mx-auto w-full max-w-5xl">
            <EmptyState
              icon={empty.icon}
              title={empty.title}
              description={empty.description}
            />
          </div>
        </div>
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
