import { useState } from "react";
import { monogram } from "@/features/entries/entries";
import { cn } from "@/lib/cn";

function faviconUrl(host: string): string {
  return `https://www.google.com/s2/favicons?sz=64&domain=${encodeURIComponent(host)}`;
}

export function EntryFavicon({
  name,
  host,
  enabled,
  size = "md",
  className,
}: {
  name: string;
  host: string | null;
  enabled: boolean;
  size?: "sm" | "md";
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const dimension = size === "sm" ? "size-7 rounded-[8px]" : "size-9 rounded-[10px]";

  if (!host || !enabled || failed) {
    return (
      <span
        className={cn(
          "grid shrink-0 place-items-center border border-[var(--border)] bg-[var(--surface-2)] font-medium text-[var(--accent-soft)]",
          dimension,
          size === "sm" ? "text-[11px]" : "text-[13px]",
          className,
        )}
      >
        {monogram(name)}
      </span>
    );
  }

  return (
    <span
      className={cn(
        "grid shrink-0 place-items-center overflow-hidden border border-[var(--border)] bg-white/95",
        dimension,
        className,
      )}
    >
      <img
        src={faviconUrl(host)}
        alt=""
        width={64}
        height={64}
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        className="size-full object-contain p-1"
        onError={() => setFailed(true)}
      />
    </span>
  );
}
