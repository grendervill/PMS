import { KeyRound, KeySquare } from "lucide-react";
import type { Entry } from "@/features/vault/types";
import { hostFromUrl, monogram } from "@/features/entries/entries";
import { cn } from "@/lib/cn";

export function EntryCard({
  entry,
  active,
  onSelect,
}: {
  entry: Entry;
  active: boolean;
  onSelect: () => void;
}) {
  const host = entry.url ? hostFromUrl(entry.url) : null;

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={active ? "true" : undefined}
      className={cn(
        "group flex w-full items-center gap-3 border-l-2 px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--accent)]",
        active
          ? "border-[var(--seal)] bg-[var(--surface-2)]"
          : "border-transparent hover:bg-[var(--surface-2)]/60",
      )}
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[13px] font-medium text-[var(--accent-soft)]">
        {monogram(entry.name)}
      </span>

      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-[13.5px] font-medium text-[var(--text-soft)]">
            {entry.name}
          </span>
          {entry.totp ? (
            <KeyRound size={11} className="shrink-0 text-[var(--muted)]" />
          ) : null}
          {(entry.sshKeyIds ?? []).length > 0 ? (
            <KeySquare size={11} className="shrink-0 text-[var(--muted)]" />
          ) : null}
        </span>
        {entry.comment ? (
          <span className="mt-0.5 block truncate text-[11px] text-[var(--muted)]">
            {entry.comment}
          </span>
        ) : host ? (
          <span className="mt-0.5 block truncate text-[11px] text-[var(--muted)]">
            {host}
          </span>
        ) : null}
      </span>

      {entry.login ? (
        <span className="hidden max-w-[90px] shrink-0 truncate text-[10.5px] text-[var(--muted)] sm:block">
          {entry.login}
        </span>
      ) : null}
    </button>
  );
}
