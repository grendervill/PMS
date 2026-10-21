import { FileUp, KeyRound, Loader2, ShieldAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { Toggle } from "@/components/ui/Toggle";
import { useToast } from "@/components/ui/toast-context";
import { parseImportFile, runImport, type ImportResult } from "@/features/import/importService";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { ImportBundle } from "@/features/vault/api";
import { getStorage } from "@/lib/storage";

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "Файл";
}

export function ImportWizard({
  onClose,
  onImported,
}: {
  onClose: () => void;
  onImported?: (result: ImportResult) => void;
}) {
  const data = useVaultStore((state) => state.data);
  const sessionId = useVaultStore((state) => state.sessionId);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const save = useVaultStore((state) => state.save);
  const toast = useToast();
  const [path, setPath] = useState<string | null>(null);
  const [bytes, setBytes] = useState<Uint8Array | null>(null);
  const [keyfile, setKeyfile] = useState<Uint8Array | null>(null);
  const [keyfileName, setKeyfileName] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [bundle, setBundle] = useState<ImportBundle | null>(null);
  const [skipDuplicates, setSkipDuplicates] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);

  const counts = useMemo(() => {
    const items = bundle?.items ?? [];
    return {
      total: items.length,
      logins: items.filter((item) => item.kind === "login").length,
      notes: items.filter((item) => item.kind !== "login" && item.kind !== "ssh").length,
      ssh: items.filter((item) => item.kind === "ssh").length,
      attachments: items.reduce((sum, item) => sum + item.attachments.length, 0),
    };
  }, [bundle]);

  const chooseFile = async () => {
    setBusy(true);
    setError(null);
    try {
      const selected = await getStorage().pickFile("Выберите файл для импорта");
      if (!selected) return;
      const content = await getStorage().readFile(selected);
      setPath(selected);
      setBytes(content);
      setBundle(null);
      setResult(null);
    } catch (readError) {
      setError(errorText(readError));
    } finally {
      setBusy(false);
    }
  };

  const chooseKeyfile = async () => {
    setBusy(true);
    setError(null);
    try {
      const selected = await getStorage().pickFile("Выберите key-файл KeePass");
      if (!selected) return;
      setKeyfile(await getStorage().readFile(selected));
      setKeyfileName(basename(selected));
    } catch (readError) {
      setError(errorText(readError));
    } finally {
      setBusy(false);
    }
  };

  const parse = async () => {
    if (!bytes) {
      setError("Сначала выберите файл.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setBundle(await parseImportFile(bytes, password || undefined, keyfile ?? undefined));
    } catch (parseError) {
      setError(errorText(parseError));
    } finally {
      setBusy(false);
    }
  };

  const doImport = async () => {
    if (!bundle || !data || !sessionId || !vaultPath) {
      setError("Хранилище не открыто.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const imported = await runImport(bundle, data, sessionId, vaultPath, {
        skipDuplicates,
      });
      const saved = await save(imported.data);
      if (!saved) {
        setError(errorText(useVaultStore.getState().error));
        return;
      }
      setResult(imported.result);
      toast.show({
        title: "Импорт завершён",
        description: `Записей: ${imported.result.entries}, вложений: ${imported.result.attachments}`,
        tone: "success",
      });
      onImported?.(imported.result);
    } catch (importError) {
      setError(errorText(importError));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Импорт из другого менеджера"
      description="Bitwarden, 1Password, KeePass, Kaspersky, браузеры и CSV."
      className="w-[min(92vw,620px)]"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {result ? "Закрыть" : "Отмена"}
          </Button>
          {!bundle && !result ? (
            <Button onClick={() => void parse()} disabled={!bytes || busy}>
              {busy ? <Loader2 size={15} className="animate-spin" /> : <FileUp size={15} />}
              Проверить файл
            </Button>
          ) : bundle && !result ? (
            <Button onClick={() => void doImport()} disabled={busy}>
              {busy ? <Loader2 size={15} className="animate-spin" /> : <FileUp size={15} />}
              Импортировать
            </Button>
          ) : null}
        </>
      }
    >
      <div className="grid gap-4">
        <div className="flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/50 px-3 py-2.5">
          <FileUp size={15} className="shrink-0 text-[var(--accent-soft)]" />
          <span className="min-w-0 flex-1 truncate text-[11.5px] text-[var(--text-soft)]">
            {path ? basename(path) : "Файл не выбран"}
          </span>
          <Button variant="secondary" size="sm" onClick={() => void chooseFile()} disabled={busy}>
            Выбрать
          </Button>
        </div>

        <Field
          label="Пароль файла"
          htmlFor="import-password"
          hint="Для зашифрованного Bitwarden JSON или KeePass KDBX."
        >
          <SecretInput
            id="import-password"
            value={password}
            onChange={(event) => {
              setPassword(event.target.value);
              setError(null);
            }}
            placeholder="Если файл защищён"
          />
        </Field>

        <div className="flex items-center gap-2">
          <Button variant="secondary" size="sm" onClick={() => void chooseKeyfile()} disabled={busy}>
            <KeyRound size={14} />
            {keyfileName ? basename(keyfileName) : "Key-файл KeePass"}
          </Button>
          {keyfile ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setKeyfile(null);
                setKeyfileName(null);
              }}
            >
              Убрать
            </Button>
          ) : null}
        </div>

        {bundle ? (
          <div className="rounded-[var(--radius-md)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 p-3.5">
            <p className="text-[12px] font-medium text-[var(--accent-soft)]">
              Формат: {bundle.format}
            </p>
            <p className="mt-1 text-[11px] leading-relaxed text-[var(--text-soft)]">
              Записей: {counts.total} · логинов: {counts.logins} · заметок/карт:{" "}
              {counts.notes} · SSH: {counts.ssh} · вложений: {counts.attachments}
            </p>
            <div className="mt-3">
              <Toggle
                checked={skipDuplicates}
                onCheckedChange={setSkipDuplicates}
                label="Пропускать точные дубликаты"
                description="Совпадение имени, логина, пароля и ссылок."
              />
            </div>
          </div>
        ) : null}

        {bundle?.warnings.length ? (
          <div className="max-h-32 overflow-y-auto rounded-[var(--radius-sm)] border border-[var(--warning)]/25 bg-[var(--warning)]/5 px-3 py-2.5">
            <p className="flex items-center gap-1.5 text-[10.5px] font-medium text-[var(--warning)]">
              <ShieldAlert size={12} />
              Предупреждения
            </p>
            <ul className="mt-1 grid gap-0.5 text-[10.5px] leading-relaxed text-[var(--text-soft)]">
              {bundle.warnings.slice(0, 20).map((warning, index) => (
                <li key={`${warning}-${index}`}>• {warning}</li>
              ))}
            </ul>
          </div>
        ) : null}

        {result ? (
          <div className="rounded-[var(--radius-sm)] border border-[var(--accent)]/25 bg-[var(--accent)]/5 p-3 text-[11px] leading-relaxed text-[var(--text-soft)]">
            Импортировано записей: {result.entries}, SSH: {result.sshKeys}, вложений:{" "}
            {result.attachments}. Пропущено: {result.skipped}.
          </div>
        ) : null}

        {error ? (
          <p role="alert" className="text-[11.5px] leading-relaxed text-[var(--danger)]">
            {error}
          </p>
        ) : null}

        <p className="text-[10px] leading-relaxed text-[var(--muted)]/75">
          Файл импорта не копируется в хранилище. После импорта удалите исходный
          plaintext-экспорт.
        </p>
      </div>
    </Modal>
  );
}
