import { KeyRound } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/EmptyState";
import { EntryDetail } from "@/features/entries/EntryDetail";
import { EntryEditor } from "@/features/entries/EntryEditor";
import { EntryList } from "@/features/entries/EntryList";
import { HistoryDialog } from "@/features/entries/HistoryDialog";
import {
  searchEntries,
  sortEntries,
  type SortMode,
} from "@/features/entries/entries";
import { useVaultStore } from "@/features/vault/store";
import type { Entry } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function EntriesView() {
  const data = useVaultStore((state) => state.data);
  const deleteEntry = useVaultStore((state) => state.deleteEntry);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortMode>("name");
  const [editor, setEditor] = useState<{ entry: Entry | null } | null>(null);
  const [historyEntry, setHistoryEntry] = useState<Entry | null>(null);

  const entries = useMemo(() => data?.entries ?? [], [data?.entries]);
  const visibleEntries = useMemo(
    () => sortEntries(searchEntries(entries, query), sort),
    [entries, query, sort],
  );
  const selected = entries.find((entry) => entry.id === selectedId) ?? null;

  const remove = async (entry: Entry) => {
    if (!window.confirm(`Удалить запись «${entry.name}»? Это действие необратимо.`)) return;
    const ok = await deleteEntry(entry.id);
    if (ok && selectedId === entry.id) setSelectedId(null);
  };

  const onSaved = () => {
    if (editor?.entry) return;
    const created = useVaultStore.getState().data?.entries[0];
    if (created) setSelectedId(created.id);
  };

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <section
        className={cn(
          "min-h-0 w-full flex-col border-r border-[var(--border)] bg-[var(--surface)]/35 lg:flex lg:w-[380px] lg:shrink-0",
          selected ? "hidden" : "flex",
        )}
      >
        <EntryList
          entries={visibleEntries}
          total={entries.length}
          selectedId={selectedId}
          query={query}
          sort={sort}
          onQuery={setQuery}
          onSort={setSort}
          onSelect={setSelectedId}
          onCreate={() => setEditor({ entry: null })}
        />
      </section>

      <section
        className={cn(
          "min-h-0 min-w-0 flex-1 flex-col",
          selected ? "flex" : "hidden lg:flex",
        )}
      >
        {selected ? (
          <EntryDetail
            entry={selected}
            onBack={() => setSelectedId(null)}
            onEdit={() => setEditor({ entry: selected })}
            onHistory={() => setHistoryEntry(selected)}
            onDelete={() => void remove(selected)}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center p-6">
            <EmptyState
              icon={KeyRound}
              title="Выберите запись"
              description="Данные появятся здесь: логин, пароль, комментарий и история изменений."
              className="w-full max-w-xl"
            />
          </div>
        )}
      </section>

      {editor ? (
        <EntryEditor
          entry={editor.entry}
          onClose={() => setEditor(null)}
          onSaved={onSaved}
        />
      ) : null}

      {historyEntry ? (
        <HistoryDialog
          key={historyEntry.id}
          entry={historyEntry}
          open
          onOpenChange={(open) => {
            if (!open) setHistoryEntry(null);
          }}
        />
      ) : null}
    </div>
  );
}
