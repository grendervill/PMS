import { File, FileText, Image as ImageIcon, Paperclip } from "lucide-react";
import { useMemo, useState } from "react";
import { AttachmentPreviewModal } from "@/features/attachments/AttachmentPreviewModal";
import { formatBytes } from "@/features/attachments/mime";
import { useVaultStore } from "@/features/vault/store";
import type { AttachmentMeta } from "@/features/vault/types";

function kindIcon(kind: AttachmentMeta["kind"]) {
  if (kind === "image") return ImageIcon;
  if (kind === "text") return FileText;
  return File;
}

export function AttachmentsSection({
  attachmentIds,
}: {
  attachmentIds: string[];
}) {
  const data = useVaultStore((state) => state.data);
  const [preview, setPreview] = useState<AttachmentMeta | null>(null);
  const attachments = useMemo(() => {
    const byId = new Map((data?.attachments ?? []).map((item) => [item.id, item]));
    return attachmentIds
      .map((id) => byId.get(id))
      .filter((item): item is AttachmentMeta => item !== undefined);
  }, [data?.attachments, attachmentIds]);

  if (attachments.length === 0) return null;

  return (
    <div className="border-t border-[var(--border)] py-4">
      <div className="mb-2 flex items-center gap-2">
        <Paperclip size={13} className="text-[var(--muted)]" />
        <span className="text-[11.5px] text-[var(--muted)]">
          Вложения · {attachments.length}
        </span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        {attachments.map((item) => {
          const Icon = kindIcon(item.kind);
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => setPreview(item)}
              className="group flex min-w-0 items-center gap-2.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/60 px-3 py-2.5 text-left transition-colors hover:border-[var(--border-strong)] hover:bg-[var(--surface-2)]"
            >
              <span className="grid size-8 shrink-0 place-items-center rounded-[7px] border border-[var(--border)] bg-[var(--surface)] text-[var(--accent-soft)]">
                <Icon size={15} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12px] text-[var(--text-soft)]">
                  {item.name}
                </span>
                <span className="mt-0.5 block text-[10px] text-[var(--muted)]">
                  {formatBytes(item.size)}
                </span>
              </span>
            </button>
          );
        })}
      </div>

      {preview ? (
        <AttachmentPreviewModal
          key={preview.id}
          meta={preview}
          onClose={() => setPreview(null)}
        />
      ) : null}
    </div>
  );
}
