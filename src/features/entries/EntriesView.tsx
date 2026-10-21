import { KeyRound } from "lucide-react";
import { QrImportDialog } from "@/features/qr/QrImportDialog";
import { useEffect, useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/EmptyState";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { EntryDetail } from "@/features/entries/EntryDetail";
import { EntryEditor } from "@/features/entries/EntryEditor";
import { EntryList } from "@/features/entries/EntryList";
import { HistoryDialog } from "@/features/entries/HistoryDialog";
import {
  searchEntries,
  sortEntries,
  type EntryDraft,
  type SortMode,
} from "@/features/entries/entries";
import { useVaultStore } from "@/features/vault/store";
import { errorText } from "@/features/vault/errors";
import type { Entry } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function EntriesView({
  initialEntryId,
  initialDraft,
  onRequestLock,
}: {
  initialEntryId?: string | null;
  initialDraft?: { id: string; payload: EntryDraft } | null;
  onRequestLock?: () => void;
}) {
  const data = useVaultStore((state) => state.data);
  const deleteEntry = useVaultStore((state) => state.deleteEntry);
  const confirm = useConfirm();
  const toast = useToast();
  const entries = useMemo(() => data?.entries ?? [], [data?.entries]);
  const [selectedId, setSelectedId] = useState<string | null>(
    initialDraft?.id ?? initialEntryId ?? null,
  );
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortMode>("name");
  const [editor, setEditor] = useState<{
    entry: Entry | null;
    initialDraft?: EntryDraft;
    draftId?: string;
  } | null>(() => {
    if (!initialDraft) return null;
    const entry = entries.find((item) => item.id === initialDraft.id) ?? null;
    return {
      entry,
      initialDraft: initialDraft.payload,
      draftId: initialDraft.id,
    };
  });
  const [historyEntry, setHistoryEntry] = useState<Entry | null>(null);
  const [qrImportOpen, setQrImportOpen] = useState(false);

  useEffect(() => {
    const onBack = (event: Event) => {
      if (qrImportOpen) {
        setQrImportOpen(false);
        event.preventDefault();
        return;
      }
      if (historyEntry) {
        setHistoryEntry(null);
        event.preventDefault();
        return;
      }
      if (editor) {
        setEditor(null);
        event.preventDefault();
        return;
      }
      if (selectedId) {
        setSelectedId(null);
        event.preventDefault();
      }
    };
    window.addEventListener("pms:android-back", onBack);
    return () => window.removeEventListener("pms:android-back", onBack);
  }, [qrImportOpen, historyEntry, editor, selectedId]);

  const visibleEntries = useMemo(
    () => sortEntries(searchEntries(entries, query), sort),
    [entries, query, sort],
  );
  const selected = entries.find((entry) => entry.id === selectedId) ?? null;

  const remove = async (entry: Entry) => {
    const allowed = await confirm({
      title: `Удалить запись «${entry.name}»?`,
      description: "Это действие необратимо. Резервная копия хранилища останется.",
      confirmLabel: "Удалить",
      tone: "danger",
    });
    if (!allowed) return;

    const ok = await deleteEntry(entry.id);
    if (ok && selectedId === entry.id) setSelectedId(null);
    if (!ok) {
      toast.show({
        title: "Не удалось удалить запись",
        description: errorText(useVaultStore.getState().error),
        tone: "error",
      });
    }
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
          onScanQr={() => setQrImportOpen(true)}
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
            key={selected.id}
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
          initialDraft={editor.initialDraft}
          draftId={editor.draftId}
          onClose={() => setEditor(null)}
          onSaved={onSaved}
          onRequestLock={onRequestLock}
        />
      ) : null}

      {qrImportOpen ? (
        <QrImportDialog
          onClose={() => setQrImportOpen(false)}
          onImported={(id) => {
            if (id) setSelectedId(id);
          }}
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
