import { useMemo, useState } from "react";
import {
  ArrowLeft,
  FileKey,
  FolderOpen,
  KeyRound,
  Loader2,
  Lock,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { IconButton } from "@/components/ui/IconButton";
import { SecretInput } from "@/components/ui/SecretInput";
import { toBase64 } from "@/lib/base64";
import { getStorage } from "@/lib/storage";
import { useVaultStore } from "@/features/vault/store";
import type { KdfSlotKind, UnlockSecret } from "@/features/vault/types";
import { cn } from "@/lib/cn";

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "Хранилище";
}

function errorMessage(code: string, fallback: string) {
  switch (code) {
    case "invalid_credentials":
      return "Не удалось разблокировать. Проверьте пароль-фразу или ключ-файл.";
    case "keyfile_passphrase_required":
      return "У ключа есть парольная фраза. Введите её.";
    case "keyfile_passphrase_invalid":
      return "Неверная парольная фраза ключа.";
    case "keyfile_public_key":
      return "Это публичный ключ. Выберите приватный ключ-файл.";
    case "unsupported_keyfile":
      return "Этот формат ключ-файла пока не поддерживается. Используйте OpenSSH или незашифрованный PEM/PKCS#8.";
    case "truncated":
    case "bad_header":
    case "bad_payload":
    case "bad_magic":
      return "Файл хранилища повреждён или это не хранилище PMS.";
    default:
      return fallback;
  }
}

export function UnlockScreen() {
  const publicInfo = useVaultStore((state) => state.publicInfo);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const busy = useVaultStore((state) => state.busy);
  const error = useVaultStore((state) => state.error);
  const unlock = useVaultStore((state) => state.unlock);
  const reset = useVaultStore((state) => state.reset);
  const clearError = useVaultStore((state) => state.clearError);

  const slotKinds = useMemo<KdfSlotKind[]>(
    () => publicInfo?.slotKinds ?? ["passphrase"],
    [publicInfo],
  );
  const [method, setMethod] = useState<KdfSlotKind>(slotKinds[0] ?? "passphrase");
  const effectiveMethod = slotKinds.includes(method)
    ? method
    : (slotKinds[0] ?? "passphrase");
  const [passphrase, setPassphrase] = useState("");
  const [keyfileName, setKeyfileName] = useState<string | null>(null);
  const [keyfileBytes, setKeyfileBytes] = useState<Uint8Array | null>(null);
  const [keyfilePassphrase, setKeyfilePassphrase] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const [readingKeyfile, setReadingKeyfile] = useState(false);

  const chooseMethod = (kind: KdfSlotKind) => {
    setMethod(kind);
    setLocalError(null);
    clearError();
  };

  const chooseKeyfile = async () => {
    setReadingKeyfile(true);
    setLocalError(null);
    clearError();
    try {
      const path = await getStorage().pickFile();
      if (!path) return;
      const bytes = await getStorage().readFile(path);
      setKeyfileBytes(bytes);
      setKeyfileName(basename(path));
    } catch {
      setLocalError("Не удалось прочитать ключ-файл.");
    } finally {
      setReadingKeyfile(false);
    }
  };

  const submit = async () => {
    setLocalError(null);
    let secret: UnlockSecret;

    if (effectiveMethod === "passphrase") {
      if (!passphrase) {
        setLocalError("Введите пароль-фразу.");
        return;
      }
      secret = { kind: "passphrase", value: passphrase };
    } else {
      if (!keyfileBytes) {
        setLocalError("Выберите ключ-файл.");
        return;
      }
      secret = {
        kind: "keyfile",
        bytesB64: toBase64(keyfileBytes),
        passphrase: keyfilePassphrase || undefined,
      };
    }

    await unlock(secret);
  };

  const message = localError
    ? localError
    : error
      ? errorMessage(error.code, error.message)
      : null;

  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-[var(--bg)] px-4 py-8 text-[var(--text)]">
      <div className="pointer-events-none absolute inset-0">
        <div className="absolute left-1/2 top-[-18rem] size-[36rem] -translate-x-1/2 rounded-full border border-[var(--border)]/40" />
        <div className="absolute left-1/2 top-[-14rem] size-[28rem] -translate-x-1/2 rounded-full border border-[var(--border)]/30" />
      </div>

      <div className="relative w-full max-w-[430px]">
        <div className="mb-6 flex items-center justify-between">
          <button
            type="button"
            onClick={() => void reset()}
            className="flex items-center gap-1.5 text-[11.5px] text-[var(--muted)] transition-colors hover:text-[var(--text)]"
          >
            <ArrowLeft size={14} />
            Другое хранилище
          </button>
          <div className="flex items-center gap-1.5 text-[10.5px] text-[var(--muted)]">
            <ShieldCheck size={13} className="text-[var(--accent-soft)]" />
            Локально
          </div>
        </div>

        <div className="overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/90 shadow-[var(--shadow-float)]">
          <div className="border-b border-[var(--border)] px-6 pb-5 pt-6 text-center">
            <div className="mx-auto grid size-12 place-items-center rounded-[14px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent)]">
              <Lock size={22} strokeWidth={1.7} />
            </div>
            <h1 className="pms-display mt-4 text-[26px] leading-tight">Открыть хранилище</h1>
            <p className="mt-1 truncate text-[11.5px] text-[var(--muted)]">
              {publicInfo ? basename(vaultPath ?? "") : "Выберите хранилище"}
            </p>
            {vaultPath ? (
              <p className="mx-auto mt-1 max-w-[330px] truncate text-[10px] text-[var(--muted)]/65">
                {vaultPath}
              </p>
            ) : null}
          </div>

          <div className="px-6 py-5">
            {slotKinds.length > 1 ? (
              <div className="mb-5 grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
                {slotKinds.map((kind) => (
                  <button
                    key={kind}
                    type="button"
                    onClick={() => chooseMethod(kind)}
                    className={cn(
                      "flex h-8 items-center justify-center gap-1.5 rounded-[6px] text-[11.5px] transition-colors",
                      effectiveMethod === kind
                        ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                        : "text-[var(--muted)] hover:text-[var(--text-soft)]",
                    )}
                  >
                    {kind === "passphrase" ? <KeyRound size={13} /> : <FileKey size={13} />}
                    {kind === "passphrase" ? "Пароль-фраза" : "Ключ-файл"}
                  </button>
                ))}
              </div>
            ) : null}

            <form
              onSubmit={(event) => {
                event.preventDefault();
                void submit();
              }}
            >
              {effectiveMethod === "passphrase" ? (
                <Field label="Пароль-фраза" htmlFor="unlock-passphrase">
                  <SecretInput
                    id="unlock-passphrase"
                    value={passphrase}
                    onChange={(event) => {
                      setPassphrase(event.target.value);
                      setLocalError(null);
                      clearError();
                    }}
                    placeholder="Введите пароль-фразу"
                    autoFocus
                  />
                </Field>
              ) : (
                <div className="flex flex-col gap-3">
                  {keyfileName ? (
                    <div className="flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2.5">
                      <FileKey size={15} className="shrink-0 text-[var(--accent-soft)]" />
                      <span className="min-w-0 flex-1 truncate text-[12px] text-[var(--text-soft)]">
                        {keyfileName}
                      </span>
                      <IconButton
                        label="Убрать ключ-файл"
                        size="sm"
                        onClick={() => {
                          setKeyfileName(null);
                          setKeyfileBytes(null);
                          setKeyfilePassphrase("");
                        }}
                      >
                        <Trash2 size={14} />
                      </IconButton>
                    </div>
                  ) : (
                    <Button
                      variant="secondary"
                      onClick={() => void chooseKeyfile()}
                      disabled={readingKeyfile}
                    >
                      {readingKeyfile ? (
                        <Loader2 size={15} className="animate-spin" />
                      ) : (
                        <FolderOpen size={15} />
                      )}
                      Выбрать ключ-файл
                    </Button>
                  )}

                  <Field
                    label="Парольная фраза ключа"
                    htmlFor="unlock-keyfile-passphrase"
                    hint="Если у SSH-ключа есть своя парольная фраза."
                  >
                    <SecretInput
                      id="unlock-keyfile-passphrase"
                      value={keyfilePassphrase}
                      onChange={(event) => {
                        setKeyfilePassphrase(event.target.value);
                        setLocalError(null);
                        clearError();
                      }}
                      placeholder="Если есть"
                    />
                  </Field>
                </div>
              )}

              {message ? (
                <p
                  role="alert"
                  className="mt-3 rounded-[var(--radius-sm)] border border-[var(--danger)]/25 bg-[var(--danger)]/8 px-3 py-2 text-[11.5px] leading-relaxed text-[var(--danger)]"
                >
                  {message}
                </p>
              ) : null}

              <Button
                type="submit"
                className="mt-5 h-10 w-full"
                disabled={busy || readingKeyfile}
              >
                {busy ? (
                  <Loader2 size={15} className="animate-spin" />
                ) : (
                  <Lock size={15} />
                )}
                {busy ? "Открываем…" : "Разблокировать"}
              </Button>
            </form>

            <p className="mt-4 text-center text-[10px] leading-relaxed text-[var(--muted)]/70">
              Пароль-фраза не хранится. Ключ-файл остаётся на вашем устройстве.
            </p>
          </div>
        </div>
      </div>
    </main>
  );
}
