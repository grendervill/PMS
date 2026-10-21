import {
  ArrowLeft,
  Calendar,
  Download,
  Eye,
  EyeOff,
  Fingerprint,
  KeySquare,
  Loader2,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { IconButton } from "@/components/ui/IconButton";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { SshInstallDialog } from "@/features/ssh/SshInstallDialog";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { SshKey } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";
import { isAndroidPlatform } from "@/lib/platform";

function typeLabel(key: SshKey) {
  if (key.type === "rsa") return `RSA ${key.rsaBits ?? 2048}`;
  if (key.type === "ecdsa") return `ECDSA ${(key.curve ?? "p256").toUpperCase()}`;
  return "Ed25519";
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString("ru-RU", {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

function safeFileName(value: string) {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9._-]/g, "") || "ssh_key"
  );
}

export function SshDetail({
  sshKey,
  onBack,
  onDelete,
  onOpenEntry,
}: {
  sshKey: SshKey;
  onBack: () => void;
  onDelete: () => void;
  onOpenEntry?: (id: string) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const toast = useToast();
  const confirm = useConfirm();
  const [revealed, setRevealed] = useState(false);
  const [fingerprint, setFingerprint] = useState<string | null>(sshKey.fingerprint ?? null);
  const [fingerprintError, setFingerprintError] = useState(false);
  const [installOpen, setInstallOpen] = useState(false);
  const android = isAndroidPlatform();
  const [downloading, setDownloading] = useState<"openssh" | "pem" | "pkcs8" | null>(null);

  const linkedEntries = useMemo(
    () => (data?.entries ?? []).filter((entry) => (entry.sshKeyIds ?? []).includes(sshKey.id)),
    [data?.entries, sshKey.id],
  );

  useEffect(() => {
    if (sshKey.fingerprint) return;
    let active = true;
    void vaultApi
      .sshFormats(sshKey.privateKey, sshKey.passphrase ?? undefined)
      .then((formats) => {
        if (active) setFingerprint(formats.fingerprint);
      })
      .catch(() => {
        if (active) setFingerprintError(true);
      });
    return () => {
      active = false;
    };
  }, [sshKey.privateKey, sshKey.passphrase, sshKey.fingerprint]);

  useEffect(() => {
    if (!revealed) return;
    const timer = setTimeout(() => setRevealed(false), 30_000);
    return () => clearTimeout(timer);
  }, [revealed]);

  const download = async (kind: "openssh" | "pem" | "pkcs8") => {
    setDownloading(kind);
    try {
      const formats = await vaultApi.sshFormats(sshKey.privateKey, sshKey.passphrase ?? undefined);
      const extension = kind === "openssh" ? "key" : kind === "pem" ? "pem" : "p8";
      const path = await getStorage().pickSaveFile(
        `${safeFileName(sshKey.name)}.${extension}`,
        "text/plain",
      );
      if (!path) return;
      const body =
        kind === "openssh" ? formats.openssh : kind === "pem" ? formats.pem : formats.pkcs8;
      await getStorage().saveFile(path, new TextEncoder().encode(body));
      toast.show({ title: "Файл сохранён", description: path, tone: "success" });
    } catch (error) {
      toast.show({
        title: "Не удалось сохранить формат",
        description: errorText(error),
        tone: "error",
      });
    } finally {
      setDownloading(null);
    }
  };

  const confirmDelete = async () => {
    const allowed = await confirm({
      title: `Удалить ключ «${sshKey.name}»?`,
      description: "Ключ исчезнет из хранилища и из привязанных записей. Установленные файлы останутся на диске.",
      confirmLabel: "Удалить",
      tone: "danger",
    });
    if (allowed) onDelete();
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
            <div className="grid size-12 shrink-0 place-items-center rounded-[13px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
              <KeySquare size={21} />
            </div>
            <div className="min-w-0">
              <h2 className="pms-display break-words text-[27px] leading-tight tracking-tight">
                {sshKey.name}
              </h2>
              <div className="mt-1.5 flex flex-wrap items-center gap-2">
                <Badge tone="accent">{typeLabel(sshKey)}</Badge>
                {sshKey.passphrase ? <Badge tone="warning">С парольной фразой</Badge> : null}
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1">
            {!android ? (
              <IconButton label="Установить на устройство" onClick={() => setInstallOpen(true)}>
                <Download size={15} />
              </IconButton>
            ) : null}
            <IconButton label="Удалить ключ" onClick={() => void confirmDelete()} className="hover:text-[var(--danger)]">
              <Trash2 size={15} />
            </IconButton>
          </div>
        </div>
      </div>

      <div className="flex-1 px-5 py-5 sm:px-7">
        <div className="mx-auto max-w-2xl divide-y divide-[var(--border)]">
          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Fingerprint</span>
            <div className="flex min-w-0 items-center gap-1.5">
              {fingerprint ? (
                <>
                  <Fingerprint size={13} className="shrink-0 text-[var(--accent-soft)]" />
                  <span className="pms-mono min-w-0 break-all text-[11.5px] text-[var(--text-soft)]">
                    {fingerprint}
                  </span>
                </>
              ) : fingerprintError ? (
                <span className="text-[11.5px] text-[var(--warning)]">
                  Не удалось прочитать ключ. Проверьте парольную фразу.
                </span>
              ) : (
                <span className="text-[11.5px] text-[var(--muted)]">Вычисляем…</span>
              )}
            </div>
          </div>

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Публичный ключ</span>
            <div className="flex min-w-0 items-start gap-1.5">
              <span className="pms-mono min-w-0 break-all text-[11.5px] leading-relaxed text-[var(--text-soft)]">
                {sshKey.publicKey}
              </span>
              <CopyButton value={sshKey.publicKey} label="Копировать публичный ключ" />
            </div>
          </div>

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Приватный ключ</span>
            <div className="flex min-w-0 flex-col gap-2">
              <div className="flex items-start gap-1.5">
                <div className="pms-mono max-h-56 min-w-0 flex-1 overflow-auto whitespace-pre-wrap break-all rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--bg)] p-2.5 text-[10.5px] leading-relaxed text-[var(--text-soft)]">
                  {revealed ? sshKey.privateKey : "••••••••  скрыт  ••••••••"}
                </div>
                <div className="flex shrink-0 gap-1">
                  <IconButton
                    label={revealed ? "Скрыть приватный ключ" : "Показать приватный ключ"}
                    size="sm"
                    onClick={() => {
                      if (revealed) {
                        setRevealed(false);
                        return;
                      }
                      void confirm({
                        title: "Показать приватный ключ?",
                        description: "Убедитесь, что рядом нет посторонних. Ключ скроется через 30 секунд.",
                        confirmLabel: "Показать",
                      }).then((allowed) => {
                        if (allowed) setRevealed(true);
                      });
                    }}
                  >
                    {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
                  </IconButton>
                  <CopyButton value={sshKey.privateKey} label="Копировать приватный ключ" />
                </div>
              </div>
              <p className="text-[10px] text-[var(--muted)]">
                Хранится в зашифрованном vault. Установка на диск — только по кнопке.
              </p>
            </div>
          </div>

          {sshKey.passphrase ? (
            <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
              <span className="text-[11.5px] text-[var(--muted)]">Парольная фраза</span>
              <span className="flex items-center gap-1.5 text-[11.5px] text-[var(--accent-soft)]">
                <ShieldCheck size={13} />
                Сохранена в зашифрованном виде
              </span>
            </div>
          ) : null}

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Комментарий</span>
            <p className="whitespace-pre-wrap break-words text-[12.5px] leading-relaxed text-[var(--text-soft)]">
              {sshKey.comment || <span className="text-[var(--muted)]/60">Нет комментария</span>}
            </p>
          </div>

          <div className="grid gap-1.5 py-4 sm:grid-cols-[120px_1fr] sm:gap-4">
            <span className="text-[11.5px] text-[var(--muted)]">Записи</span>
            {linkedEntries.length > 0 ? (
              <div className="flex flex-wrap gap-1.5">
                {linkedEntries.map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    onClick={() => onOpenEntry?.(entry.id)}
                    className="rounded-[var(--radius-pill)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 px-2.5 py-1 text-[11px] text-[var(--accent-soft)] transition-colors hover:bg-[var(--accent)]/10"
                  >
                    {entry.name}
                  </button>
                ))}
              </div>
            ) : (
              <span className="text-[11.5px] text-[var(--muted)]/60">Пока не привязан к записям</span>
            )}
          </div>
        </div>

        <div className="mx-auto mt-4 max-w-2xl rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/50 p-3.5">
          <p className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--text-soft)]">
            <Download size={14} className="text-[var(--accent-soft)]" />
            Форматы и установка
          </p>
          <p className="mt-1 text-[10.5px] leading-relaxed text-[var(--muted)]">
            {android
              ? "OpenSSH хранится в vault. Скачайте ключ в нужном формате и передайте его приложению, которому он нужен."
              : "OpenSSH хранится в vault. PEM/PKCS#8 можно скачать, а ключ — установить в выбранную папку."}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            {!android ? (
              <Button size="sm" onClick={() => setInstallOpen(true)}>
                <Download size={14} />
                Установить
              </Button>
            ) : null}
            {android ? (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void download("openssh")}
                disabled={downloading !== null}
              >
                {downloading === "openssh" ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Download size={14} />
                )}
                Скачать OpenSSH
              </Button>
            ) : null}
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void download("pem")}
              disabled={downloading !== null}
            >
              {downloading === "pem" ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
              Скачать PEM
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void download("pkcs8")}
              disabled={downloading !== null}
            >
              {downloading === "pkcs8" ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
              Скачать PKCS#8
            </Button>
          </div>
        </div>
      </div>

      <div className="border-t border-[var(--border)] px-5 py-3.5 sm:px-7">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="flex items-center gap-1.5 text-[10.5px] text-[var(--muted)]">
            <Calendar size={12} />
            Изменено {formatDate(sshKey.updatedAt)}
          </span>
          {!android ? (
            <Button size="sm" variant="secondary" onClick={() => setInstallOpen(true)}>
              <Download size={14} />
              Установить на устройство
            </Button>
          ) : null}
        </div>
      </div>

      {installOpen && !android ? (
        <SshInstallDialog key={sshKey.id} sshKey={sshKey} onClose={() => setInstallOpen(false)} />
      ) : null}
    </div>
  );
}
