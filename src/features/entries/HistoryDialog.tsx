import { Eye, EyeOff, History, KeyRound } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { IconButton } from "@/components/ui/IconButton";
import { Modal } from "@/components/ui/Modal";
import type { Entry, HistoryItem } from "@/features/vault/types";
import { cn } from "@/lib/cn";

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function HistoryRow({ item, secret }: { item: HistoryItem; secret: boolean }) {
  const [revealed, setRevealed] = useState(false);

  return (
    <div className="flex items-center gap-2 border-b border-[var(--border)]/70 px-4 py-3 last:border-b-0">
      <div className="min-w-0 flex-1">
        <p className="pms-mono break-all text-[12.5px] text-[var(--text-soft)]">
          {secret && !revealed ? "•".repeat(Math.min(24, item.value.length)) : item.value}
        </p>
        <p className="mt-0.5 text-[10px] text-[var(--muted)]">{formatDate(item.changedAt)}</p>
      </div>
      {secret ? (
        <IconButton
          label={revealed ? "Скрыть" : "Показать"}
          size="sm"
          onClick={() => setRevealed((current) => !current)}
        >
          {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
        </IconButton>
      ) : null}
      <CopyButton value={item.value} label="Копировать значение" />
    </div>
  );
}

export function HistoryDialog({
  entry,
  open,
  onOpenChange,
}: {
  entry: Entry;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [tab, setTab] = useState<"password" | "login">(
    entry.passwordHistory.length > 0 ? "password" : "login",
  );
  const items = tab === "password" ? entry.passwordHistory : entry.loginHistory;

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title="История записи"
      description={entry.name}
      footer={
        <Button variant="secondary" size="sm" onClick={() => onOpenChange(false)}>
          Закрыть
        </Button>
      }
    >
      <div className="mb-4 grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
        {(
          [
            ["password", "Пароли", entry.passwordHistory.length, KeyRound],
            ["login", "Логины", entry.loginHistory.length, History],
          ] as const
        ).map(([value, label, count, Icon]) => (
          <button
            key={value}
            type="button"
            onClick={() => setTab(value)}
            className={cn(
              "flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors",
              tab === value
                ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                : "text-[var(--muted)] hover:text-[var(--text-soft)]",
            )}
          >
            <Icon size={13} />
            {label}
            <span className="text-[10px] text-[var(--muted)]">{count}</span>
          </button>
        ))}
      </div>

      {items.length > 0 ? (
        <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)]">
          {items.map((item) => (
            <HistoryRow key={`${item.changedAt}-${item.value}`} item={item} secret={tab === "password"} />
          ))}
        </div>
      ) : (
        <p className="py-8 text-center text-[12px] text-[var(--muted)]">
          История пока пуста. Здесь появятся старые значения после изменений.
        </p>
      )}

      <p className="mt-4 text-[10.5px] leading-relaxed text-[var(--muted)]/75">
        Старые пароли доступны только для просмотра и копирования. Повторно
        использовать их в этой записи нельзя.
      </p>
    </Modal>
  );
}
