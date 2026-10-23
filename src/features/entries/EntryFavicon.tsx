import { useEffect, useState } from "react";
import { monogram } from "@/features/entries/entries";
import { loadFavicon } from "@/features/entries/faviconCache";
import { cn } from "@/lib/cn";

type LoadedIcon = {
  host: string;
  dataUrl: string | null;
};

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
  const [loaded, setLoaded] = useState<LoadedIcon | null>(null);
  const [broken, setBroken] = useState<string | null>(null);
  const dimension = size === "sm" ? "size-7 rounded-[8px]" : "size-9 rounded-[10px]";

  useEffect(() => {
    if (!host || !enabled) return;
    let active = true;
    void loadFavicon(host).then((dataUrl) => {
      if (active) setLoaded({ host, dataUrl });
    });
    return () => {
      active = false;
    };
  }, [host, enabled]);

  const current = host && enabled && loaded?.host === host ? loaded : null;
  const dataUrl = current?.dataUrl && current.dataUrl !== broken ? current.dataUrl : null;

  return (
    <span
      className={cn(
        "grid shrink-0 place-items-center border border-[var(--border)]",
        dimension,
        dataUrl
          ? "overflow-hidden bg-white/95"
          : "bg-[var(--surface-2)] font-medium text-[var(--accent-soft)]",
        !dataUrl && (size === "sm" ? "text-[11px]" : "text-[13px]"),
        className,
      )}
    >
      {dataUrl ? (
        <img
          src={dataUrl}
          alt=""
          width={64}
          height={64}
          decoding="async"
          className="size-full object-contain p-1"
          onError={() => setBroken(dataUrl)}
        />
      ) : (
        monogram(name)
      )}
    </span>
  );
}
