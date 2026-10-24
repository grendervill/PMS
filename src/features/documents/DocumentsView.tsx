import { IdCard, Paperclip, Plus, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { VaultDocument } from "@/features/vault/types";
import { DocumentDetail } from "./DocumentDetail";
import { DocumentEditor } from "./DocumentEditor";
import {
  documentChip,
  documentLabel,
  documentTitle,
  maskDocumentNumber,
} from "./documentModel";

function DocumentTile({
  document,
  onSelect,
}: {
  document: VaultDocument;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={documentLabel(document)}
      onClick={onSelect}
      className="flex min-h-[140px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/55 p-4 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] hover:border-[var(--border-strong)]"
    >
      <span className="w-fit rounded-full border border-[var(--accent)]/30 bg-[var(--accent)]/8 px-2.5 py-1 text-[10.5px] text-[var(--accent-soft)]">
        {documentChip(document)}
      </span>
      <span className="mt-4 block text-[16px] font-medium text-[var(--text-soft)]">
        {documentTitle(document)}
      </span>
      <span className="pms-mono mt-1 block text-[12px] text-[var(--muted)]">
        {maskDocumentNumber(document)}
      </span>
      <span className="mt-auto flex items-center gap-1.5 pt-3 text-[10.5px] text-[var(--muted)]">
        <Paperclip size={12} />
        Вложений: {document.attachmentIds.length}
      </span>
    </button>
  );
}

export function DocumentsView() {
  const data = useVaultStore((state) => state.data);
  const deleteDocument = useVaultStore((state) => state.deleteDocument);
  const confirm = useConfirm();
  const toast = useToast();
  const documents = useMemo(() => data?.documents ?? [], [data?.documents]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<VaultDocument | "new" | null>(null);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return documents;
    return documents.filter((document) =>
      [
        documentTitle(document),
        documentChip(document),
        document.number,
        document.series,
        document.fullName,
        document.issuedBy,
        document.categories,
      ].some((value) => value.toLowerCase().includes(needle)),
    );
  }, [documents, query]);

  const selected = documents.find((document) => document.id === selectedId) ?? null;

  useEffect(() => {
    const onBack = (event: Event) => {
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
  }, [editor, selectedId]);

  const remove = async (document: VaultDocument) => {
    const allowed = await confirm({
      title: `Удалить документ «${documentLabel(document)}»?`,
      description: "Это действие необратимо. Резервная копия хранилища останется.",
      confirmLabel: "Удалить",
      tone: "danger",
    });
    if (!allowed) return;

    const ok = await deleteDocument(document.id);
    if (ok && selectedId === document.id) setSelectedId(null);
    if (!ok) {
      toast.show({
        title: "Не удалось удалить документ",
        description: errorText(useVaultStore.getState().error),
        tone: "error",
      });
    }
  };

  const onSaved = () => {
    if (editor !== "new") return;
    const created = useVaultStore.getState().data?.documents[0];
    if (created) setSelectedId(created.id);
  };

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="mx-auto w-full max-w-5xl">
        <div className="mb-4 flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative min-w-0 flex-1">
            <Search
              size={14}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--muted)]"
            />
            <Input
              aria-label="Поиск по документам"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Поиск по типу, номеру или ФИО"
              className="pl-9"
            />
          </div>
          <Button className="w-full sm:w-auto" onClick={() => setEditor("new")}>
            <Plus size={15} />
            Добавить документ
          </Button>
        </div>

        {documents.length === 0 ? (
          <EmptyState
            icon={IdCard}
            title="Пока нет документов"
            description="Добавьте паспорт, СНИЛС или водительское удостоверение — оригиналы можно приложить вложениями."
            className="min-h-[320px]"
          />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={Search}
            title="Ничего не найдено"
            description="Попробуйте изменить запрос."
            className="min-h-[260px]"
          />
        ) : (
          <div className="grid gap-4 sm:grid-cols-2">
            {visible.map((document) => (
              <DocumentTile
                key={document.id}
                document={document}
                onSelect={() => setSelectedId(document.id)}
              />
            ))}
          </div>
        )}
      </div>

      {selected ? (
        <DocumentDetail
          key={selected.id}
          document={selected}
          onClose={() => setSelectedId(null)}
          onEdit={() => {
            setEditor(selected);
            setSelectedId(null);
          }}
          onDelete={() => void remove(selected)}
        />
      ) : null}

      {editor ? (
        <DocumentEditor
          document={editor === "new" ? null : editor}
          onClose={() => setEditor(null)}
          onSaved={onSaved}
        />
      ) : null}
    </div>
  );
}
