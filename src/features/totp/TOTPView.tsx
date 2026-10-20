import { ChevronRight, Timer } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { monogram } from "@/features/entries/entries";
import { TotpCode } from "@/features/totp/TotpCode";
import { useVaultStore } from "@/features/vault/store";

export function TOTPView({ onOpenEntry }: { onOpenEntry: (id: string) => void }) {
  const data = useVaultStore((state) => state.data);
  const [query, setQuery] = useState("");
  const entries = useMemo(
    () =>
      (data?.entries ?? []).filter(
        (entry) =>
          entry.totp &&
          (entry.name.toLowerCase().includes(query.toLowerCase()) ||
            entry.login.toLowerCase().includes(query.toLowerCase())),
      ),
    [data?.entries, query],
  );

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-3xl">
        <div className="relative mb-4 max-w-sm">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Поиск по TOTP"
            className="h-9 text-[12px]"
            aria-label="Поиск по TOTP"
          />
        </div>

        {entries.length > 0 ? (
          <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/50">
            {entries.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center gap-3 border-b border-[var(--border)] px-4 py-3 last:border-b-0"
              >
                <span className="grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[12px] text-[var(--accent-soft)]">
                  {monogram(entry.name)}
                </span>
                <button
                  type="button"
                  onClick={() => onOpenEntry(entry.id)}
                  className="group min-w-0 flex-1 text-left"
                >
                  <span className="block truncate text-[13px] text-[var(--text-soft)]">
                    {entry.name}
                  </span>
                  <span className="mt-0.5 block truncate text-[10.5px] text-[var(--muted)]">
                    {entry.totp?.issuer || entry.login || "TOTP"}
                  </span>
                </button>
                {entry.totp ? <TotpCode config={entry.totp} compact /> : null}
                <ChevronRight size={14} className="text-[var(--muted)]" />
              </div>
            ))}
          </div>
        ) : (
          <EmptyState
            icon={Timer}
            title={query ? "Ничего не найдено" : "TOTP пока нет"}
            description={
              query
                ? "Попробуйте другой запрос."
                : "Добавьте TOTP-секрет в карточку — коды появятся здесь."
            }
          />
        )}

        <p className="mt-5 text-[10px] leading-relaxed text-[var(--muted)]/70">
          Коды считаются локально по RFC 6238 и не отправляются в сеть.
        </p>
      </div>
    </div>
  );
}
