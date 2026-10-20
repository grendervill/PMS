import { AlertTriangle, ChevronRight, ShieldCheck, ShieldQuestion } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/EmptyState";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import { groupDuplicateEntries, monogram } from "@/features/entries/entries";
import { useVaultStore } from "@/features/vault/store";
import type { Entry } from "@/features/vault/types";

type WeakEntry = { entry: Entry; strength: PasswordStrength };

export function AuditView({ onOpenEntry }: { onOpenEntry: (id: string) => void }) {
  const data = useVaultStore((state) => state.data);
  const entries = useMemo(() => data?.entries ?? [], [data?.entries]);
  const [weak, setWeak] = useState<WeakEntry[]>([]);
  const [scanning, setScanning] = useState(false);
  const hasPasswords = entries.some((entry) => entry.password);
  const visibleWeak = hasPasswords ? weak : [];

  const duplicateGroups = useMemo(() => groupDuplicateEntries(entries), [entries]);

  useEffect(() => {
    let cancelled = false;
    const withPasswords = entries.filter((entry) => entry.password);
    if (withPasswords.length === 0) return;

    const timer = setTimeout(() => {
      setScanning(true);
      void Promise.all(
        withPasswords.map(async (entry) => ({
          entry,
          strength: await passwordStrength(entry.password),
        })),
      ).then((results) => {
        if (cancelled) return;
        setWeak(results.filter((result) => result.strength.score <= 2));
        setScanning(false);
      });
    }, 0);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [entries]);

  const totalIssues = duplicateGroups.length + visibleWeak.length;

  if (entries.length === 0) {
    return (
      <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
        <div className="mx-auto w-full max-w-4xl">
          <EmptyState
            icon={ShieldCheck}
            title="Пока нечего проверять"
            description="Добавьте записи — тогда здесь появятся слабые и повторяющиеся пароли."
          />
        </div>
      </div>
    );
  }

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-3xl">
        <div className="mb-5 grid grid-cols-2 gap-3">
          <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3">
            <p className="text-[10.5px] text-[var(--muted)]">Повторы</p>
            <p className="pms-display mt-1 text-[28px] leading-none">
              {duplicateGroups.length}
            </p>
            <p className="mt-1 text-[10px] text-[var(--muted)]">групп с одинаковым паролем</p>
          </div>
          <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/60 px-4 py-3">
            <p className="text-[10.5px] text-[var(--muted)]">Слабые</p>
            <p className="pms-display mt-1 text-[28px] leading-none">
              {scanning ? "…" : visibleWeak.length}
            </p>
            <p className="mt-1 text-[10px] text-[var(--muted)]">записей стоит укрепить</p>
          </div>
        </div>

        {totalIssues === 0 && !scanning ? (
          <EmptyState
            icon={ShieldCheck}
            title="Всё спокойно"
            description="Повторяющихся и слабых паролей не найдено. Это не проверка утечек — только локальный анализ."
          />
        ) : null}

        {duplicateGroups.length > 0 ? (
          <section className="mb-6">
            <div className="mb-2 flex items-center gap-2">
              <AlertTriangle size={14} className="text-[var(--warning)]" />
              <h2 className="text-[12px] font-medium text-[var(--text-soft)]">
                Одинаковые пароли
              </h2>
            </div>
            <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/50">
              {duplicateGroups.map((group, index) => (
                <div key={index} className="border-b border-[var(--border)] last:border-b-0">
                  <div className="flex items-center gap-2 border-b border-[var(--border)]/60 bg-[var(--warning)]/5 px-3 py-2">
                    <span className="pms-mono text-[11px] text-[var(--warning)]">
                      ••••••
                    </span>
                    <span className="text-[10.5px] text-[var(--muted)]">
                      используется в {group.length} записях
                    </span>
                  </div>
                  {group.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      onClick={() => onOpenEntry(entry.id)}
                      className="group flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-[var(--surface-2)]"
                    >
                      <span className="grid size-7 place-items-center rounded-[7px] border border-[var(--border)] bg-[var(--surface-2)] text-[11px] text-[var(--accent-soft)]">
                        {monogram(entry.name)}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-[12px] text-[var(--text-soft)]">
                        {entry.name}
                      </span>
                      <ChevronRight
                        size={13}
                        className="text-[var(--muted)] transition-transform group-hover:translate-x-0.5"
                      />
                    </button>
                  ))}
                </div>
              ))}
            </div>
          </section>
        ) : null}

        {visibleWeak.length > 0 ? (
          <section>
            <div className="mb-2 flex items-center gap-2">
              <ShieldQuestion size={14} className="text-[var(--warning)]" />
              <h2 className="text-[12px] font-medium text-[var(--text-soft)]">Слабые пароли</h2>
            </div>
            <div className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface)]/50">
              {visibleWeak.map(({ entry, strength }) => (
                <button
                  key={entry.id}
                  type="button"
                  onClick={() => onOpenEntry(entry.id)}
                  className="group flex w-full items-center gap-3 border-b border-[var(--border)] px-3 py-2.5 text-left transition-colors last:border-b-0 hover:bg-[var(--surface-2)]"
                >
                  <span className="grid size-7 place-items-center rounded-[7px] border border-[var(--border)] bg-[var(--surface-2)] text-[11px] text-[var(--accent-soft)]">
                    {monogram(entry.name)}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12px] text-[var(--text-soft)]">
                      {entry.name}
                    </span>
                    <span className="mt-0.5 block text-[10px] text-[var(--warning)]">
                      {strength.label}
                    </span>
                  </span>
                  <ChevronRight
                    size={13}
                    className="text-[var(--muted)] transition-transform group-hover:translate-x-0.5"
                  />
                </button>
              ))}
            </div>
          </section>
        ) : scanning ? (
          <p className="py-6 text-center text-[11.5px] text-[var(--muted)]">
            Проверяем пароли локально…
          </p>
        ) : null}

        <p className="mt-6 text-[10px] leading-relaxed text-[var(--muted)]/70">
          Проверка выполняется на устройстве и не отправляет пароли в сеть.
        </p>
      </div>
    </div>
  );
}
