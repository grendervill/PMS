import type { ReactNode } from "react";

export function DetailRow({
  label,
  value,
  action,
}: {
  label: string;
  value: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3 border-b border-[var(--border)]/70 py-2 last:border-b-0">
      <span className="shrink-0 text-[11px] text-[var(--muted)]">{label}</span>
      <span className="flex min-w-0 items-center gap-1 text-right text-[12px] text-[var(--text-soft)]">
        <span className="min-w-0 break-words">{value}</span>
        {action}
      </span>
    </div>
  );
}
