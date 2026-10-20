import { Lock } from "lucide-react";
import { cn } from "@/lib/cn";
import { navItems, type AppSection } from "@/features/vault/navigation";

export type VaultSidebarProps = {
  active: AppSection;
  onNavigate: (section: AppSection) => void;
  vaultName?: string;
  status?: "demo" | "locked" | "unlocked";
  onLock?: () => void;
  className?: string;
};

export function VaultSidebar({
  active,
  onNavigate,
  vaultName,
  status = "demo",
  onLock,
  className,
}: VaultSidebarProps) {
  return (
    <aside
      className={cn(
        "relative hidden w-60 shrink-0 flex-col border-r border-[var(--border)] bg-[var(--surface)]/55 lg:flex",
        className,
      )}
    >
      <div className="pms-seam pointer-events-none absolute inset-y-0 right-0 w-px" />

      <div className="flex items-center gap-3 px-5 pb-5 pt-6">
        <div className="grid size-9 place-items-center rounded-[10px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent)]">
          <Lock size={17} strokeWidth={1.8} />
        </div>
        <div className="min-w-0">
          <p className="pms-display text-[19px] leading-none tracking-tight text-[var(--text)]">
            PMS
          </p>
          <p className="mt-1 text-[11px] text-[var(--muted)]">Личное хранилище</p>
        </div>
      </div>

      <nav aria-label="Основная навигация" className="flex flex-1 flex-col gap-0.5 px-3 pt-2">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
              className={cn(
                "flex h-10 items-center gap-3 rounded-r-[var(--radius-sm)] border-l-2 py-2 pl-3 pr-2 text-left text-[13.5px] transition-colors duration-[var(--motion-fast)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
                isActive
                  ? "border-[var(--seal)] bg-[var(--surface-2)] text-[var(--text)]"
                  : "border-transparent text-[var(--muted)] hover:bg-[var(--surface-2)]/70 hover:text-[var(--text)]",
              )}
            >
              <Icon
                size={17}
                strokeWidth={1.8}
                className={isActive ? "text-[var(--accent-soft)]" : "text-[var(--muted)]"}
              />
              <span className="truncate">{item.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="border-t border-[var(--border)] px-4 pb-5 pt-4">
        <div className="flex items-start gap-2.5">
          <span
            className={cn(
              "mt-1.5 size-2 shrink-0 rounded-full",
              status === "unlocked"
                ? "bg-[var(--accent)]"
                : status === "locked"
                  ? "bg-[var(--warning)]"
                  : "bg-[var(--muted)]/50",
            )}
          />
          <div className="min-w-0">
            <p className="truncate text-[12.5px] font-medium text-[var(--text-soft)]">
              {status === "unlocked"
                ? (vaultName ?? "Хранилище")
                : status === "locked"
                  ? "Заблокировано"
                  : "Хранилище не подключено"}
            </p>
            <p className="mt-0.5 text-[10.5px] leading-snug text-[var(--muted)]">
              {status === "unlocked"
                ? "Данные расшифрованы"
                : status === "locked"
                  ? "Введите пароль-фразу"
                  : "Создайте хранилище, чтобы начать"}
            </p>
          </div>
        </div>

        {status === "unlocked" && onLock ? (
          <button
            type="button"
            onClick={onLock}
            className="mt-3 flex h-8 w-full items-center justify-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] text-[12px] text-[var(--muted)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--text)]"
          >
            <Lock size={13} />
            Заблокировать
          </button>
        ) : null}
      </div>
    </aside>
  );
}
