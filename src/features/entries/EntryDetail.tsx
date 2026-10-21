import {
  ArrowLeft,
  Calendar,
  ExternalLink,
  Eye,
  EyeOff,
  History,
  KeySquare,
  Pencil,
  QrCode,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { IconButton } from "@/components/ui/IconButton";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { AttachmentsSection } from "@/features/attachments/AttachmentsSection";
import { ShareCardDialog } from "@/features/qr/ShareCardDialog";
import { TotpCode } from "@/features/totp/TotpCode";
import { TotpQrDialog } from "@/features/totp/TotpQrDialog";
import { hostFromUrl, monogram, safeHttpUrl, entryUrls } from "@/features/entries/entries";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { Entry } from "@/features/vault/types";
function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString("ru-RU", {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

export function EntryDetail({
  entry,
  onBack,
  onEdit,
  onDelete,
  onHistory,
}: {
  entry: Entry;
  onBack: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onHistory: () => void;
}) {
  const [revealed, setRevealed] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [totpQrOpen, setTotpQrOpen] = useState(false);
  const confirm = useConfirm();
  const toast = useToast();
  const data = useVaultStore((state) => state.data);
  const urls = entryUrls(entry);
  const primaryUrl = urls[0] ?? "";
  const host = primaryUrl ? hostFromUrl(primaryUrl) : "";
  const linkedSshKeys = (data?.sshKeys ?? []).filter((key) =>
    (entry.sshKeyIds ?? []).includes(key.id),
  );

  useEffect(() => {
    if (!revealed) return;
    const timer = setTimeout(() => setRevealed(false), 30_000);
    return () => clearTimeout(timer);
  }, [revealed]);

  const openSite = async (value: string) => {
    const httpUrl = safeHttpUrl(value);
    if (!httpUrl) return;
    const allowed = await confirm({
      title: "Открыть сайт?",
      description: hostFromUrl(value) || httpUrl,
      confirmLabel: "Открыть",
    });
    if (!allowed) return;
    try {
      await openUrl(httpUrl);
    } catch (error) {
      toast.show({
        title: "Не удалось открыть сайт",
        description: errorText(error),
        tone: "error",
      });
    }
  };

  return (
    <div className="pms-scrollbar flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="border-b border-[var(--border)] px-5 py-5 sm:px-7">
        <div className="flex items-start justify-between gap-4">
          <div className="flex min-w-0 items-start gap-3.5">
            <button
              type="button"
              onClick={onBack}
              className="mt-1 flex size-8 shrink-0 items-center justify-center rounded-[var(--radius-sm)] text-[var(--muted)] hover:bg-[var(--surface-2)] hover:text-[var(--text)] lg:hidden"
              aria-label="Назад к списку"
            >
              <ArrowLeft size={16} />
            </button>
            <div className="grid size-12 shrink-0 place-items-center rounded-[13px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-lg font-medium text-[var(--accent-soft)]">
              {monogram(entry.name)}
            </div>
            <div className="min-w-0">
              <h2 className="pms-display break-words text-[27px] leading-tight tracking-tight">
                {entry.name}
              </h2>
              {host ? (
                <button
                  type="button"
                  onClick={() => void openSite(primaryUrl)}
                  className="mt-1 flex max-w-full items-center gap-1.5 text-[11.5px] text-[var(--muted)] transition-colors hover:text-[var(--accent-soft)]"
                >
                  <span className="truncate">{host}</span>
                  <ExternalLink size={11} className="shrink-0" />
                </button>
              ) : null}
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1">
            <IconButton label="Поделиться по QR" onClick={() => setShareOpen(true)}>
              <QrCode size={15} />
            </IconButton>
            <IconButton label="Редактировать" onClick={onEdit}>
              <Pencil size={15} />
            </IconButton>
            <IconButton label="История" onClick={onHistory}>
              <History size={15} />
            </IconButton>
            <IconButton label="Удалить" onClick={onDelete} className="hover:text-[var(--danger)]">
              <Trash2 size={15} />
            </IconButton>
          </div>
        </div>
      </div>

      <div className="flex-1 px-5 py-5 sm:px-7">
        <div className="mx-auto max-w-2xl divide-y divide-[var(--border)]">
          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Логин</span>
            {entry.login ? (
              <div className="flex min-w-0 items-center gap-1.5">
                <span className="pms-mono min-w-0 break-all text-[13px] text-[var(--text-soft)]">
                  {entry.login}
                </span>
                <CopyButton value={entry.login} label="Копировать логин" />
              </div>
            ) : (
              <span className="text-[12.5px] text-[var(--muted)]/60">Не указан</span>
            )}
          </div>

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Пароль</span>
            <div className="flex min-w-0 items-center gap-1.5">
              {entry.password ? (
                <>
                  <span className="pms-mono min-w-0 break-all text-[13px] text-[var(--text-soft)]">
                    {revealed ? entry.password : "•".repeat(Math.min(32, entry.password.length))}
                  </span>
                  <IconButton
                    label={revealed ? "Скрыть пароль" : "Показать пароль"}
                    size="sm"
                    onClick={() => setRevealed((current) => !current)}
                  >
                    {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
                  </IconButton>
                  <CopyButton value={entry.password} label="Копировать пароль" />
                </>
              ) : (
                <span className="text-[12.5px] text-[var(--muted)]/60">Не указан</span>
              )}
            </div>
          </div>

          {urls.length > 0 ? (
            <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
              <span className="text-[11.5px] text-[var(--muted)]">Сайты</span>
              <div className="grid min-w-0 gap-1.5">
                {urls.map((url, index) => {
                  const httpUrl = safeHttpUrl(url);
                  return httpUrl ? (
                    <button
                      key={`${url}-${index}`}
                      type="button"
                      onClick={() => void openSite(url)}
                      className="flex min-w-0 items-center gap-1.5 text-left text-[12.5px] text-[var(--accent-soft)] hover:underline"
                    >
                      <span className="truncate">{url}</span>
                      <ExternalLink size={12} className="shrink-0" />
                    </button>
                  ) : (
                    <span
                      key={`${url}-${index}`}
                      className="min-w-0 break-all text-[12.5px] text-[var(--text-soft)]"
                    >
                      {url}
                    </span>
                  );
                })}
              </div>
            </div>
          ) : null}

          {entry.totp ? (
            <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
              <span className="text-[11.5px] text-[var(--muted)]">TOTP</span>
              <div className="flex items-center gap-2">
                <TotpCode config={entry.totp} />
                <IconButton label="Показать TOTP QR" size="sm" onClick={() => setTotpQrOpen(true)}>
                  <QrCode size={14} />
                </IconButton>
              </div>
            </div>
          ) : null}

          {linkedSshKeys.length > 0 ? (
            <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
              <span className="text-[11.5px] text-[var(--muted)]">SSH-ключи</span>
              <div className="flex flex-wrap gap-1.5">
                {linkedSshKeys.map((key) => (
                  <span
                    key={key.id}
                    className="inline-flex items-center gap-1.5 rounded-[var(--radius-pill)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 px-2.5 py-1 text-[11px] text-[var(--accent-soft)]"
                  >
                    <KeySquare size={11} />
                    {key.name}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Комментарий</span>
            <p className="whitespace-pre-wrap break-words text-[12.5px] leading-relaxed text-[var(--text-soft)]">
              {entry.comment || <span className="text-[var(--muted)]/60">Нет комментария</span>}
            </p>
          </div>
        </div>
        <AttachmentsSection entry={entry} />
      </div>

      {shareOpen ? (
        <ShareCardDialog entry={entry} onClose={() => setShareOpen(false)} />
      ) : null}
      {totpQrOpen && entry.totp ? (
        <TotpQrDialog config={entry.totp} onClose={() => setTotpQrOpen(false)} />
      ) : null}

      <div className="border-t border-[var(--border)] px-5 py-3.5 sm:px-7">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="flex items-center gap-1.5 text-[10.5px] text-[var(--muted)]">
            <Calendar size={12} />
            Изменено {formatDate(entry.updatedAt)}
          </span>
          <Button size="sm" variant="secondary" onClick={onEdit}>
            <Pencil size={14} />
            Редактировать
          </Button>
        </div>
      </div>
    </div>
  );
}
