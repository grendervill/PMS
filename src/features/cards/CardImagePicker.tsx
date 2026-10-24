import { Check, ImagePlus, Loader2, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { IconButton } from "@/components/ui/IconButton";
import { basename, formatBytes, mimeFromName } from "@/features/attachments/mime";
import { errorText } from "@/features/vault/errors";
import { vaultApi } from "@/features/vault/api";
import { useVaultStore } from "@/features/vault/store";
import type { CardImagePreset } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";
import { CARD_IMAGE_PRESETS } from "./cardModel";
import { cn } from "@/lib/cn";

export const MAX_CARD_IMAGE_BYTES = 10 * 1024 * 1024;
const CARD_IMAGE_MIMES = new Set(["image/png", "image/jpeg", "image/webp"]);

export function CardImagePicker({
  preset,
  attachmentId,
  imageUrl,
  onPreset,
  onAttachment,
  onError,
}: {
  preset: CardImagePreset;
  attachmentId: string | null;
  imageUrl: string | null;
  onPreset: (preset: CardImagePreset) => void;
  onAttachment: (id: string | null) => void;
  onError: (message: string) => void;
}) {
  const sessionId = useVaultStore((state) => state.sessionId);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const addAttachmentMeta = useVaultStore((state) => state.addAttachmentMeta);
  const [busy, setBusy] = useState(false);

  const upload = async () => {
    setBusy(true);
    try {
      const path = await getStorage().pickFile("Выберите изображение карты");
      if (!path) return;

      const name = basename(path);
      const mime = mimeFromName(name);
      if (!CARD_IMAGE_MIMES.has(mime)) {
        onError("Выберите картинку в формате PNG, JPEG или WebP");
        return;
      }

      const bytes = await getStorage().readFile(path);
      if (bytes.length > MAX_CARD_IMAGE_BYTES) {
        onError(`Картинка больше ${formatBytes(MAX_CARD_IMAGE_BYTES)}`);
        return;
      }
      if (!sessionId || !vaultPath) {
        onError("Хранилище не разблокировано");
        return;
      }

      const sealed = await vaultApi.sealAttachment(sessionId, name, mime, bytes);
      await getStorage().writeBlob(vaultPath, sealed.meta.id, sealed.blob);
      const saved = await addAttachmentMeta(sealed.meta);
      if (!saved) {
        await getStorage().deleteBlob(vaultPath, sealed.meta.id).catch(() => undefined);
        throw new Error("Не удалось сохранить метаданные картинки");
      }
      onAttachment(sealed.meta.id);
    } catch (error) {
      onError(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/45 p-3.5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-[12.5px] font-medium text-[var(--text-soft)]">
            Оформление карты
          </p>
          <p className="mt-1 text-[10.5px] leading-relaxed text-[var(--muted)]">
            Пресет или своя картинка. Своя картинка хранится в зашифрованном виде.
          </p>
        </div>
        {attachmentId ? (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void upload()}
            disabled={busy}
          >
            {busy ? <Loader2 size={14} className="animate-spin" /> : <ImagePlus size={14} />}
            Заменить
          </Button>
        ) : null}
      </div>

      {attachmentId && imageUrl ? (
        <div className="mt-3 flex items-center gap-2.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface)] p-2">
          <img
            src={imageUrl}
            alt=""
            className="h-12 w-20 shrink-0 rounded-[6px] object-cover"
          />
          <div className="min-w-0 flex-1">
            <p className="truncate text-[11.5px] text-[var(--text-soft)]">
              Своя картинка
            </p>
            <p className="mt-0.5 text-[10px] text-[var(--muted)]">
              Используется вместо пресета
            </p>
          </div>
          <IconButton
            label="Убрать картинку"
            size="sm"
            onClick={() => onAttachment(null)}
          >
            <Trash2 size={14} />
          </IconButton>
        </div>
      ) : null}

      <div className="mt-3 grid grid-cols-3 gap-2 sm:grid-cols-6">
        {CARD_IMAGE_PRESETS.map((option) => {
          const active = option.id === preset && !attachmentId;
          return (
            <button
              key={option.id}
              type="button"
              aria-label={option.label}
              aria-pressed={active}
              onClick={() => onPreset(option.id)}
              className={cn(
                "group relative overflow-hidden rounded-[8px] border p-0 transition-[border-color,transform] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]",
                active
                  ? "border-[var(--accent)]"
                  : "border-[var(--border)] hover:border-[var(--border-strong)]",
              )}
            >
              <span className="pms-card-swatch block" data-preset={option.id} />
              {active ? (
                <span className="absolute right-1 top-1 grid size-4 place-items-center rounded-full bg-[var(--accent)] text-[#08110d]">
                  <Check size={10} strokeWidth={3} />
                </span>
              ) : null}
            </button>
          );
        })}
      </div>

      {!attachmentId ? (
        <Button
          variant="secondary"
          size="sm"
          className="mt-3 w-full"
          onClick={() => void upload()}
          disabled={busy}
        >
          {busy ? <Loader2 size={14} className="animate-spin" /> : <ImagePlus size={14} />}
          {busy ? "Загружаем…" : "Загрузить картинку"}
        </Button>
      ) : null}
    </div>
  );
}
