import { Plus, Search } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { EntryCard } from "@/features/entries/EntryCard";
import type { SortMode } from "@/features/entries/entries";
import type { Entry } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function EntryList({
  entries,
  total,
  selectedId,
  query,
  sort,
  onQuery,
  onSort,
  onSelect,
  onCreate,
}: {
  entries: Entry[];
  total: number;
  selectedId: string | null;
  query: string;
  sort: SortMode;
  onQuery: (value: string) => void;
  onSort: (value: SortMode) => void;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-[var(--border)] px-4 py-3.5">
        <div className="flex items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <Search
              size={14}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--muted)]"
            />
            <Input
              aria-label="Поиск по записям"
              value={query}
              onChange={(event) => onQuery(event.target.value)}
              placeholder="Поиск по записям"
              className="h-8 pl-8 text-[12px]"
            />
          </div>
          <Button size="sm" onClick={onCreate}>
            <Plus size={14} />
            Запись
          </Button>
        </div>

        <div className="mt-2.5 flex items-center justify-between">
          <span className="text-[10.5px] text-[var(--muted)]">
            {total === 0 ? "Нет записей" : `${total} ${total === 1 ? "запись" : "записей"}`}
          </span>
          <div className="flex items-center gap-0.5 rounded-[6px] border border-[var(--border)] bg-[var(--surface)] p-0.5">
            {(
              [
                ["name", "По имени"],
                ["updated", "По дате"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => onSort(value)}
                className={cn(
                  "rounded-[4px] px-2 py-0.5 text-[10px] transition-colors",
                  sort === value
                    ? "bg-[var(--surface-2)] text-[var(--text-soft)]"
                    : "text-[var(--muted)] hover:text-[var(--text-soft)]",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto">
        {entries.length > 0 ? (
          <div className="divide-y divide-[var(--border)]/70">
            {entries.map((entry) => (
              <EntryCard
                key={entry.id}
                entry={entry}
                active={entry.id === selectedId}
                onSelect={() => onSelect(entry.id)}
              />
            ))}
          </div>
        ) : (
          <div className="p-4">
            <EmptyState
              icon={Search}
              title={query ? "Ничего не найдено" : "Пока нет записей"}
              description={
                query
                  ? "Попробуйте другое слово или очистите поиск."
                  : "Создайте первую запись — логин, пароль и всё, что нужно помнить."
              }
              action={
                query ? (
                  <Button variant="secondary" size="sm" onClick={() => onQuery("")}>
                    Очистить поиск
                  </Button>
                ) : (
                  <Button size="sm" onClick={onCreate}>
                    <Plus size={14} />
                    Создать запись
                  </Button>
                )
              }
              className="min-h-[240px]"
            />
          </div>
        )}
      </div>
    </div>
  );
}
