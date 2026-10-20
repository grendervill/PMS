import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export type BadgeTone = "neutral" | "accent" | "warning" | "danger";

const toneClass: Record<BadgeTone, string> = {
  neutral: "border-[var(--border)] bg-[var(--surface-2)] text-[var(--muted)]",
  accent: "border-[var(--accent)]/30 bg-[var(--accent)]/10 text-[var(--accent-soft)]",
  warning: "border-[var(--warning)]/30 bg-[var(--warning)]/10 text-[var(--warning)]",
  danger: "border-[var(--danger)]/30 bg-[var(--danger)]/10 text-[var(--danger)]",
};

export function Badge({
  children,
  tone = "neutral",
  className,
}: {
  children: ReactNode;
  tone?: BadgeTone;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex h-5 items-center rounded-[var(--radius-pill)] border px-2 text-[11px] font-medium leading-none",
        toneClass[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
