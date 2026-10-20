import { useEffect, useState } from "react";
import { CopyButton } from "@/components/ui/CopyButton";
import { vaultApi } from "@/features/vault/api";
import type { TotpConfig } from "@/features/vault/types";
import { cn } from "@/lib/cn";

function formatCode(code: string) {
  if (code.length === 6) return `${code.slice(0, 3)} ${code.slice(3)}`;
  if (code.length === 8) return `${code.slice(0, 4)} ${code.slice(4)}`;
  return code;
}

export function TotpCode({
  config,
  compact = false,
}: {
  config: TotpConfig;
  compact?: boolean;
}) {
  const [code, setCode] = useState("");
  const [remaining, setRemaining] = useState<number>(config.period);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const tick = async () => {
      try {
        const result = await vaultApi.totpCode(config);
        if (cancelled) return;
        setCode(result.code);
        setRemaining(result.remaining);
        setError(false);
      } catch {
        if (!cancelled) setError(true);
      }
    };

    void tick();
    const timer = setInterval(() => void tick(), 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [config]);

  const progress = Math.max(0, Math.min(100, (remaining / config.period) * 100));

  if (compact) {
    return (
      <span className="inline-flex items-center gap-2">
        <span className="pms-mono text-[12px] tracking-[0.08em] text-[var(--text-soft)]">
          {error ? "—" : formatCode(code)}
        </span>
        <span
          className={cn(
            "text-[9.5px] tabular-nums",
            remaining <= 5 ? "text-[var(--warning)]" : "text-[var(--muted)]",
          )}
        >
          {remaining} с
        </span>
      </span>
    );
  }

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2.5">
      <span className="pms-mono text-[18px] tracking-[0.12em] text-[var(--text)]">
        {error ? "—" : formatCode(code)}
      </span>
      <div className="h-1 w-20 overflow-hidden rounded-full bg-[var(--surface-3)]">
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-1000 ease-linear",
            remaining <= 5 ? "bg-[var(--warning)]" : "bg-[var(--accent)]",
          )}
          style={{ width: `${progress}%` }}
        />
      </div>
      <span className="text-[10px] tabular-nums text-[var(--muted)]">
        {remaining} с
      </span>
      {!error && code ? <CopyButton value={code} label="Копировать TOTP-код" /> : null}
    </div>
  );
}
