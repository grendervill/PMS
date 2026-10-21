import { AlertTriangle, Download, KeyRound, Loader2, QrCode, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { useToast } from "@/components/ui/toast-context";
import { entryUrls } from "@/features/entries/entries";
import { makeQrDataUrl, MAX_QR_CHARS } from "@/features/qr/qr";
import { vaultApi } from "@/features/vault/api";
import { errorText } from "@/features/vault/errors";
import type { Entry } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";

function safeName(value: string) {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9._-]/g, "") || "card"
  );
}

function dataUrlBytes(value: string): Uint8Array {
  const binary = atob(value.split(",")[1] ?? "");
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function ShareCardDialog({
  entry,
  onClose,
}: {
  entry: Entry;
  onClose: () => void;
}) {
  const toast = useToast();
  const [secure, setSecure] = useState(false);
  const [password, setPassword] = useState("");
  const [payload, setPayload] = useState<string | null>(null);
  const [image, setImage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const card = useMemo(
    () => ({
      name: entry.name,
      login: entry.login,
      password: entry.password,
      urls: entryUrls(entry),
      comment: entry.comment,
      totp: entry.totp ?? null,
    }),
    [entry],
  );

  const generate = useCallback(
    async (usePassword: boolean, passphrase: string) => {
      setBusy(true);
      setError(null);
      try {
        const encoded = await vaultApi.shareEncode(
          card,
          usePassword ? passphrase : undefined,
        );
        if (encoded.length > MAX_QR_CHARS) {
          throw { code: "qr_too_large", message: "Данные не помещаются в QR-код" };
        }
        const dataUrl = await makeQrDataUrl(encoded);
        setPayload(encoded);
        setImage(dataUrl);
      } catch (generateError) {
        setPayload(null);
        setImage(null);
        setError(errorText(generateError));
      } finally {
        setBusy(false);
      }
    },
    [card],
  );

  useEffect(() => {
    const timer = setTimeout(() => {
      void generate(false, "");
    }, 0);
    return () => clearTimeout(timer);
  }, [generate]);

  const savePng = async () => {
    if (!image) return;
    setSaving(true);
    try {
      const path = await save({
        defaultPath: `${safeName(entry.name)}-qr.png`,
      });
      if (!path) return;
      await getStorage().saveFile(path, dataUrlBytes(image));
      toast.show({ title: "QR-код сохранён", description: path, tone: "success" });
    } catch (saveError) {
      toast.show({
        title: "Не удалось сохранить QR",
        description: errorText(saveError),
        tone: "error",
      });
    } finally {
      setSaving(false);
    }
  };

  const generateSecure = () => {
    if (password.trim().length < 4) {
      setError("Пароль QR должен быть не короче 4 символов.");
      return;
    }
    void generate(true, password);
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Поделиться карточкой"
      description="Другое устройство PMS сможет импортировать эту запись по QR."
      className="w-[min(92vw,600px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            Закрыть
          </Button>
          {secure ? (
            <Button onClick={generateSecure} disabled={busy || !password}>
              {busy ? <Loader2 size={15} className="animate-spin" /> : <QrCode size={15} />}
              Создать QR
            </Button>
          ) : (
            <Button variant="secondary" onClick={() => void savePng()} disabled={!image || saving}>
              {saving ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />}
              Сохранить PNG
            </Button>
          )}
        </>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-2">
          <div className="grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
            <button
              type="button"
              onClick={() => {
                setSecure(false);
                setError(null);
                if (!image) void generate(false, "");
              }}
              className={`flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors ${
                !secure
                  ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                  : "text-[var(--muted)] hover:text-[var(--text-soft)]"
              }`}
            >
              <QrCode size={13} />
              Без пароля
            </button>
            <button
              type="button"
              onClick={() => {
                setSecure(true);
                setError(null);
                setPayload(null);
                setImage(null);
              }}
              className={`flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors ${
                secure
                  ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                  : "text-[var(--muted)] hover:text-[var(--text-soft)]"
              }`}
            >
              <KeyRound size={13} />
              С паролем
            </button>
          </div>
          <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
            {secure
              ? "Пароль понадобится ввести на принимающем устройстве. Он не сохраняется в QR."
              : "Проще всего: данные карточки лежат в QR открыто. Не показывайте код посторонним."}
          </p>
        </div>

        {secure ? (
          <Field label="Пароль QR" htmlFor="share-qr-password" hint="Не короче 4 символов.">
            <SecretInput
              id="share-qr-password"
              value={password}
              onChange={(event) => {
                setPassword(event.target.value);
                setError(null);
              }}
              placeholder="Пароль для передачи"
              autoFocus
            />
          </Field>
        ) : null}

        {secure && !image ? (
          <div className="flex items-start gap-2 rounded-[var(--radius-sm)] border border-[var(--warning)]/25 bg-[var(--warning)]/5 px-3 py-2.5 text-[11px] leading-relaxed text-[var(--text-soft)]">
            <ShieldCheck size={14} className="mt-0.5 shrink-0 text-[var(--warning)]" />
            Нажмите «Создать QR» после ввода пароля.
          </div>
        ) : null}

        {busy ? (
          <div className="flex min-h-48 items-center justify-center rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg)]/40">
            <Loader2 size={22} className="animate-spin text-[var(--accent-soft)]" />
          </div>
        ) : image ? (
          <div className="grid gap-2">
            <div className="mx-auto rounded-[var(--radius-md)] border border-[var(--border)] bg-white p-3">
              <img src={image} alt="QR-код карточки" className="size-64" />
            </div>
            {payload ? (
              <p className="text-center text-[10px] text-[var(--muted)]">
                {payload.length} из {MAX_QR_CHARS} символов
              </p>
            ) : null}
          </div>
        ) : null}

        {error ? (
          <p role="alert" className="text-[11.5px] leading-relaxed text-[var(--danger)]">
            {error}
          </p>
        ) : null}

        <div className="rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/50 px-3 py-2.5 text-[10.5px] leading-relaxed text-[var(--muted)]">
          <p className="flex items-start gap-1.5">
            <AlertTriangle size={12} className="mt-0.5 shrink-0 text-[var(--warning)]" />
            В QR передаются имя, логин, пароль, URL, комментарий и TOTP. Вложения и
            SSH-ключи не передаются.
          </p>
        </div>
      </div>
    </Modal>
  );
}
