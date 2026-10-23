import { useRef, type PointerEvent } from "react";
import type { Entry } from "@/features/vault/types";
import { entryUrls, hostFromUrl } from "@/features/entries/entries";
import { EntryFavicon } from "@/features/entries/EntryFavicon";
import type { CardLayout } from "@/features/entries/cardLayout";
import { cn } from "@/lib/cn";
import "./EntryCard.css";

export function EntryCard({
  entry,
  active,
  layout,
  faviconEnabled,
  onSelect,
}: {
  entry: Entry;
  active: boolean;
  layout: CardLayout;
  faviconEnabled: boolean;
  onSelect: () => void;
}) {
  const primaryUrl = entryUrls(entry)[0] ?? "";
  const host = primaryUrl ? hostFromUrl(primaryUrl) : null;
  const advancedRef = useRef<HTMLButtonElement>(null);

  const move = (event: PointerEvent<HTMLButtonElement>) => {
    if (layout !== "advanced") return;
    const element = advancedRef.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    element.style.setProperty("--mx", `${event.clientX - rect.left}px`);
    element.style.setProperty("--my", `${event.clientY - rect.top}px`);
  };

  if (layout === "advanced") {
    return (
      <button
        ref={advancedRef}
        type="button"
        onClick={onSelect}
        aria-current={active ? "true" : undefined}
        onPointerMove={move}
        className={cn(
          "entry-card-advanced flex w-full flex-col overflow-hidden rounded-[14px] border px-3.5 py-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
          active ? "entry-card-advanced--active" : "",
        )}
      >
        <span className="relative z-[1] flex min-w-0 flex-col items-center text-center">
          <EntryFavicon
            name={entry.name}
            host={host}
            enabled={faviconEnabled}
            size="sm"
            className="border-[var(--border-strong)] bg-[var(--surface-2)]"
          />
          <span className="mt-1.5 max-w-full truncate text-[14px] font-medium text-[var(--text-soft)]">
            {entry.name}
          </span>
          {primaryUrl ? (
            <span className="pms-mono mt-0.5 max-w-full truncate text-[9.5px] text-[var(--muted)]">
              {primaryUrl}
            </span>
          ) : null}
        </span>

        {entry.login || entry.password ? (
          <span className="relative z-[1] mt-2.5 flex min-w-0 items-center justify-between gap-3 border-t border-[var(--border)]/70 pt-2 text-[10.5px]">
            <span className="min-w-0 truncate text-[var(--muted)]">{entry.login}</span>
            {entry.password ? (
              <span className="pms-mono shrink-0 tracking-[0.12em] text-[var(--text-soft)]">
                •••••
              </span>
            ) : null}
          </span>
        ) : null}

        <span className="relative z-[1] mt-2 flex min-h-7 flex-1 items-start text-left text-[10.5px] leading-relaxed text-[var(--muted)]">
          <span className="line-clamp-2 break-words">{entry.comment}</span>
        </span>
      </button>
    );
  }

  if (layout === "compact") {
    return (
      <button
        type="button"
        onClick={onSelect}
        aria-current={active ? "true" : undefined}
        className={cn(
          "group flex w-full items-center gap-2.5 border-l-2 px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--accent)]",
          active
            ? "border-[var(--seal)] bg-[var(--surface-2)]"
            : "border-transparent hover:bg-[var(--surface-2)]/65",
        )}
      >
        <EntryFavicon
          name={entry.name}
          host={host}
          enabled={faviconEnabled}
          size="sm"
          className="border-transparent bg-transparent"
        />
        <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-[var(--text-soft)]">
          {entry.name}
        </span>
      </button>
    );
  }

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
      <EntryFavicon name={entry.name} host={host} enabled={faviconEnabled} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13.5px] font-medium text-[var(--text-soft)]">
          {entry.name}
        </span>
        <span className="mt-0.5 block min-h-[15px] truncate text-[11px] text-[var(--muted)]">
          {entry.comment}
        </span>
      </span>
    </button>
  );
}
