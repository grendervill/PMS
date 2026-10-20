import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/cn";

export type EmptyStateProps = {
  icon: ComponentType<{ size?: number | string; className?: string }>;
  title: string;
  description: string;
  action?: ReactNode;
  className?: string;
};

export function EmptyState({ icon: Icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex min-h-[280px] flex-col items-center justify-center rounded-[var(--radius-lg)] border border-dashed border-[var(--border)] bg-[var(--surface)]/40 px-6 py-12 text-center",
        className,
      )}
    >
      <div className="mb-4 grid size-11 place-items-center rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
        <Icon size={20} />
      </div>
      <h2 className="pms-display text-xl text-[var(--text)]">{title}</h2>
      <p className="mt-1.5 max-w-sm text-sm leading-relaxed text-[var(--muted)]">{description}</p>
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}
