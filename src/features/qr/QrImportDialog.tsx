import { AlertTriangle, KeyRound, Loader2, QrCode, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { QrScannerDialog } from "@/features/qr/QrScannerDialog";
import { vaultApi, type SharedCard } from "@/features/vault/api";
import { errorCode, errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import { useMemo } from "react";

export function QrImportDialog({
  onClose,
  onImported,
}: {
  onClose: () => void;
  onImported?: (id?: string) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const addEntry = useVaultStore((state) => state.addEntry);
  const confirm = useConfirm();
  const toast = useToast();
  const [scanning, setScanning] = useState(true);
  const [payload, setPayload] = useState<string | null>(null);
  const [needsPassword, setNeedsPassword] = useState(false);
  const [password, setPassword] = useState("");
  const [card, setCard] = useState<SharedCard | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const duplicate = useMemo(() => {
    if (!card || !data) return null;
    const urls = card.urls.join("\n");
    return (
      data.entries.find(
        (entry) =>
          entry.name === card.name &&
          entry.login === card.login &&
          entry.password === card.password &&
          (entry.urls ?? []).join("\n") === urls,
      ) ?? null
    );
  }, [card, data]);

  const decode = async (value: string, passphrase?: string) => {
    setBusy(true);
    setError(null);
    try {
      const decoded = await vaultApi.shareDecode(value, passphrase);
      setCard(decoded);
      setNeedsPassword(false);
    } catch (decodeError) {
      if (errorCode(decodeError) === "qr_password_required") {
        setNeedsPassword(true);
      } else {
        setError(errorText(decodeError));
      }
    } finally {
      setBusy(false);
    }
  };

  const onScan = (text: string) => {
    setScanning(false);
    setPayload(text);
    void decode(text);
  };

  const importCard = async () => {
    if (!card) return;
    if (duplicate) {
      const allowed = await confirm({
        title: "Похожая запись уже есть",
        description: `${duplicate.name} совпадает по логину, паролю и ссылкам. Создать копию?`,
        confirmLabel: "Создать копию",
      });
      if (!allowed) {
        onClose();
        return;
      }
    }
    const ok = await addEntry({
      name: card.name,
      login: card.login,
      password: card.password,
      urls: card.urls,
      comment: card.comment,
      attachmentIds: [],
      sshKeyIds: [],
      totp: card.totp,
      totpRaw: "",
    });
    if (ok) {
      toast.show({ title: "Карточка импортирована", tone: "success" });
      const created = useVaultStore.getState().data?.entries[0];
      onImported?.(created?.id);
      onClose();
    } else {
      setError(errorText(useVaultStore.getState().error));
    }
  };

  return (
    <>
      <Modal
        open={!scanning}
        onOpenChange={(open) => {
          if (!open) onClose();
        }}
        title="Импорт по QR"
        description="Наведите камеру или выберите скриншот QR-кода PMS."
        className="w-[min(92vw,540px)]"
        footer={
          <>
            <Button variant="ghost" onClick={onClose}>
              Отмена
            </Button>
            {card ? (
              <Button onClick={() => void importCard()} disabled={busy}>
                <QrCode size={15} />
                Импортировать
              </Button>
            ) : needsPassword ? (
              <Button
                onClick={() => payload && void decode(payload, password)}
                disabled={busy || !password}
              >
                {busy ? <Loader2 size={15} className="animate-spin" /> : <KeyRound size={15} />}
                Открыть
              </Button>
            ) : (
              <Button variant="secondary" onClick={() => setScanning(true)} disabled={busy}>
                <QrCode size={15} />
                Сканировать снова
              </Button>
            )}
          </>
        }
      >
        <div className="grid gap-4">
          {busy ? (
            <div className="flex min-h-32 items-center justify-center">
              <Loader2 size={22} className="animate-spin text-[var(--accent-soft)]" />
            </div>
          ) : card ? (
            <>
              <div className="rounded-[var(--radius-md)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 p-4">
                <div className="flex items-start gap-2">
                  <ShieldCheck size={15} className="mt-0.5 shrink-0 text-[var(--accent-soft)]" />
                  <div className="min-w-0">
                    <p className="text-[11px] text-[var(--accent-soft)]">Карточка PMS</p>
                    <h3 className="pms-display mt-0.5 break-words text-xl">{card.name}</h3>
                  </div>
                </div>
                <div className="mt-3 grid gap-2 text-[11.5px] text-[var(--text-soft)]">
                  {card.login ? (
                    <p>
                      <span className="text-[var(--muted)]">Логин: </span>
                      {card.login}
                    </p>
                  ) : null}
                  {card.password ? (
                    <p>
                      <span className="text-[var(--muted)]">Пароль: </span>
                      {"•".repeat(Math.min(24, card.password.length))}
                    </p>
                  ) : null}
                  {card.urls.length > 0 ? (
                    <div>
                      <span className="text-[var(--muted)]">Ссылки:</span>
                      <ul className="mt-1 grid gap-0.5">
                        {card.urls.map((url) => (
                          <li key={url} className="truncate text-[var(--accent-soft)]">
                            {url}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                  {card.totp ? <p className="text-[var(--accent-soft)]">TOTP включён</p> : null}
                  {card.comment ? (
                    <p className="whitespace-pre-wrap break-words text-[var(--muted)]">
                      {card.comment}
                    </p>
                  ) : null}
                </div>
              </div>
              {duplicate ? (
                <p className="flex items-start gap-1.5 text-[10.5px] leading-relaxed text-[var(--warning)]">
                  <AlertTriangle size={12} className="mt-0.5 shrink-0" />
                  Похожая запись уже есть: {duplicate.name}. При импорте спросим, создавать ли копию.
                </p>
              ) : null}
            </>
          ) : needsPassword ? (
            <Field
              label="Пароль QR"
              htmlFor="qr-import-password"
              hint="Этот QR защищён паролем, который задал отправитель."
            >
              <SecretInput
                id="qr-import-password"
                value={password}
                onChange={(event) => {
                  setPassword(event.target.value);
                  setError(null);
                }}
                placeholder="Введите пароль"
                autoFocus
              />
            </Field>
          ) : (
            <p className="text-[12px] text-[var(--muted)]">QR-код пока не распознан.</p>
          )}

          {error ? (
            <p role="alert" className="text-[11.5px] leading-relaxed text-[var(--danger)]">
              {error}
            </p>
          ) : null}
          {!card && !needsPassword ? (
            <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
              Импорт создаёт новую запись. Вложения и SSH-ключи в QR не передаются.
            </p>
          ) : null}
        </div>
      </Modal>

      {scanning ? (
        <QrScannerDialog
          open
          onOpenChange={(open) => {
            if (!open) setScanning(false);
          }}
          onResult={onScan}
          title="Импорт карточки по QR"
        />
      ) : null}
    </>
  );
}
