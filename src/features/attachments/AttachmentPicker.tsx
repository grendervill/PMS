import { File, FileText, Image as ImageIcon, Loader2, Paperclip, Plus, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { IconButton } from "@/components/ui/IconButton";
import { useToast } from "@/components/ui/toast-context";
import { basename, formatBytes, mimeFromName } from "@/features/attachments/mime";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { AttachmentMeta } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";

const MAX_BYTES = 50 * 1024 * 1024;

function KindIcon({ kind }: { kind: AttachmentMeta["kind"] }) {
  if (kind === "image") return <ImageIcon size={15} />;
  if (kind === "text") return <FileText size={15} />;
  return <File size={15} />;
}

export function AttachmentPicker({
  ids,
  onChange,
}: {
  ids: string[];
  onChange: (ids: string[]) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const sessionId = useVaultStore((state) => state.sessionId);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const addAttachmentMeta = useVaultStore((state) => state.addAttachmentMeta);
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const attachments = useMemo(() => {
    const byId = new Map((data?.attachments ?? []).map((item) => [item.id, item]));
    return ids
      .map((id) => byId.get(id))
      .filter((item): item is AttachmentMeta => item !== undefined);
  }, [data?.attachments, ids]);

  const add = async () => {
    if (!sessionId || !vaultPath) return;
    setBusy(true);
    try {
      const path = await getStorage().pickFile("Выберите вложение");
      if (!path) return;

      const name = basename(path);
      const mime = mimeFromName(name);
      const bytes = await getStorage().readFile(path);
      if (bytes.length > MAX_BYTES) {
        throw { code: "file_too_large", message: "Файл больше 50 МБ" };
      }

      const sealed = await vaultApi.sealAttachment(sessionId, name, mime, bytes);
      await getStorage().writeBlob(vaultPath, sealed.meta.id, sealed.blob);
      const saved = await addAttachmentMeta(sealed.meta);
      if (!saved) {
        await getStorage().deleteBlob(vaultPath, sealed.meta.id).catch(() => undefined);
        throw new Error("Не удалось сохранить метаданные вложения");
      }
      onChange([...ids, sealed.meta.id]);
    } catch (error) {
      toast.show({
        title: "Не удалось добавить вложение",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/50 p-3.5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="flex items-center gap-1.5 text-[12.5px] font-medium text-[var(--text-soft)]">
            <Paperclip size={13} className="text-[var(--accent-soft)]" />
            Вложения
          </p>
          <p className="mt-1 text-[10.5px] text-[var(--muted)]">
            Фото, PDF, текст или любой другой файл. До 50 МБ.
          </p>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void add()}
          disabled={busy}
        >
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Plus size={14} />}
          Добавить
        </Button>
      </div>

      {attachments.length > 0 ? (
        <div className="mt-3 grid gap-2">
          {attachments.map((item) => (
            <div
              key={item.id}
              className="flex items-center gap-2.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface)] px-2.5 py-2"
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-[6px] border border-[var(--border)] text-[var(--accent-soft)]">
                <KindIcon kind={item.kind} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[11.5px] text-[var(--text-soft)]">
                  {item.name}
                </span>
                <span className="text-[10px] text-[var(--muted)]">{formatBytes(item.size)}</span>
              </span>
              <IconButton
                label="Убрать вложение"
                size="sm"
                onClick={() => onChange(ids.filter((id) => id !== item.id))}
              >
                <Trash2 size={13} />
              </IconButton>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
