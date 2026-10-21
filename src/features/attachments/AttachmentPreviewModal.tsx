import { Download, FileQuestion, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { vaultApi } from "@/features/vault/api";
import { getStorage } from "@/lib/storage";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { AttachmentMeta } from "@/features/vault/types";
import { formatBytes } from "@/features/attachments/mime";
import { previewMime, safeDownloadName, safePreviewKind } from "@/features/attachments/preview";

export function AttachmentPreviewModal({
  meta,
  onClose,
}: {
  meta: AttachmentMeta;
  onClose: () => void;
}) {
  const sessionId = useVaultStore((state) => state.sessionId);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const toast = useToast();
  const confirm = useConfirm();
  const kind = safePreviewKind(meta);
  const [url, setUrl] = useState<string | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const bytesRef = useRef<Uint8Array | null>(null);
  const [loading, setLoading] = useState(
    (kind === "image" || kind === "pdf" || kind === "text") &&
      !(kind === "text" && meta.size > 1024 * 1024),
  );
  const [downloading, setDownloading] = useState(false);

  const loadBytes = async () => {
    if (bytesRef.current) return bytesRef.current;
    if (!sessionId || !vaultPath) throw new Error("Хранилище не разблокировано");
    const blob = await getStorage().readBlob(vaultPath, meta.id);
    const bytes = await vaultApi.openAttachment(sessionId, meta.id, blob);
    bytesRef.current = bytes;
    return bytes;
  };

  useEffect(() => {
    if (!sessionId || !vaultPath) return;
    if (kind !== "image" && kind !== "pdf" && kind !== "text") return;
    if (kind === "text" && meta.size > 1024 * 1024) return;

    let cancelled = false;
    let objectUrl: string | null = null;

    void (async () => {
      try {
        const bytes = await loadBytes();
        if (cancelled) return;
        if (kind === "text") {
          setText(new TextDecoder().decode(bytes));
        } else {
          objectUrl = URL.createObjectURL(
            new Blob([bytes as unknown as BlobPart], {
              type: previewMime(meta, kind),
            }),
          );
          setUrl(objectUrl);
        }
      } catch (loadError) {
        if (!cancelled) setError(errorText(loadError));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
    // The modal is mounted per attachment and must load once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meta.id]);

  const download = async () => {
    setDownloading(true);
    try {
      const allowed = await confirm({
        title: "Сохранить файл на диск?",
        description:
          "Файл будет расшифрован и станет доступен другим приложениям на этом устройстве.",
        confirmLabel: "Сохранить",
      });
      if (!allowed) return;
      const bytes = await loadBytes();
      const path = await save({ defaultPath: safeDownloadName(meta.name) });
      if (!path) return;
      await getStorage().saveFile(path, bytes);
      toast.show({ title: "Файл сохранён", tone: "success" });
    } catch (downloadError) {
      toast.show({
        title: "Не удалось сохранить файл",
        description: errorText(downloadError),
        tone: "error",
      });
    } finally {
      setDownloading(false);
    }
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={meta.name}
      description={`${formatBytes(meta.size)} · ${meta.mime}`}
      className="w-[min(94vw,820px)]"
      footer={
        <Button variant="secondary" size="sm" onClick={() => void download()} disabled={downloading}>
          {downloading ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
          Скачать
        </Button>
      }
    >
      {loading ? (
        <div className="flex min-h-[220px] items-center justify-center gap-2 text-[12px] text-[var(--muted)]">
          <Loader2 size={16} className="animate-spin" />
          Расшифровываем…
        </div>
      ) : error ? (
        <div className="flex min-h-[180px] flex-col items-center justify-center gap-2 text-center">
          <FileQuestion size={24} className="text-[var(--danger)]" />
          <p className="text-[12px] text-[var(--danger)]">{error}</p>
        </div>
      ) : kind === "image" && url ? (
        <div className="flex max-h-[65vh] items-center justify-center overflow-auto rounded-[var(--radius-sm)] bg-[var(--bg)] p-2">
          <img src={url} alt={meta.name} className="max-h-[62vh] max-w-full object-contain" />
        </div>
      ) : kind === "pdf" && url ? (
        <iframe
          src={url}
          title={meta.name}
          sandbox=""
          className="h-[62vh] w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-white"
        />
      ) : kind === "text" && text !== null ? (
        <pre className="pms-scrollbar max-h-[62vh] overflow-auto whitespace-pre-wrap break-words rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--bg)] p-4 text-[12px] leading-relaxed text-[var(--text-soft)]">
          {text}
        </pre>
      ) : (
        <div className="flex min-h-[180px] flex-col items-center justify-center gap-2 text-center">
          <FileQuestion size={24} className="text-[var(--muted)]" />
          <p className="text-[12px] text-[var(--muted)]">
            {kind === "text" && meta.size > 1024 * 1024
              ? "Текст больше 1 МБ — скачайте файл, чтобы открыть его."
              : "Предпросмотр для этого типа файла недоступен. Скачайте его, чтобы открыть."}
          </p>
        </div>
      )}
    </Modal>
  );
}
