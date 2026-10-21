import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  FileKey,
  Fingerprint,
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
import { vaultApi } from "@/features/vault/api";
import { isBiometricEnabled } from "@/features/vault/biometric";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { KdfSlotKind, UnlockSecret } from "@/features/vault/types";
import { cn } from "@/lib/cn";

export function UnlockScreen() {
  const publicInfo = useVaultStore((state) => state.publicInfo);
  const vaultPath = useVaultStore((state) => state.vaultPath);
  const busy = useVaultStore((state) => state.busy);
  const error = useVaultStore((state) => state.error);
  const unlock = useVaultStore((state) => state.unlock);
  const unlockBiometric = useVaultStore((state) => state.unlockBiometric);
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
  const [bioAvailable, setBioAvailable] = useState<boolean | null>(null);
  const [bioEnabled, setBioEnabled] = useState(() =>
    isBiometricEnabled(publicInfo?.vaultId),
  );
  const [bioAttemptsLeft, setBioAttemptsLeft] = useState(5);
  const [bioBusy, setBioBusy] = useState(false);
  const [bioNotice, setBioNotice] = useState<string | null>(null);
  const [bioStopped, setBioStopped] = useState(false);
  const autoTried = useRef(false);

  const vaultId = publicInfo?.vaultId;

  useEffect(() => {
    if (!vaultId) return;
    let active = true;
    void vaultApi
      .biometricStatus()
      .then((status) => {
        if (!active) return;
        setBioAvailable(status.available);
        setBioEnabled(isBiometricEnabled(vaultId) && status.available);
      })
      .catch(() => {
        if (active) {
          setBioAvailable(false);
          setBioEnabled(false);
        }
      });
    return () => {
      active = false;
    };
  }, [vaultId]);

  const attemptBiometric = useCallback(async () => {
    if (bioBusy || bioStopped || bioAttemptsLeft <= 0) return;
    setBioBusy(true);
    setBioNotice(null);
    const result = await unlockBiometric();
    setBioBusy(false);
    if (result.ok) return;

    if (result.code === "biometric_cancelled") {
      setBioStopped(true);
      setBioNotice("Вход по отпечатку отменён. Введите пароль-фразу.");
      return;
    }
    if (result.code === "biometric_failed") {
      const left = Math.max(0, bioAttemptsLeft - 1);
      setBioAttemptsLeft(left);
      if (left === 0) {
        setBioStopped(true);
        setBioNotice("Попытки закончились. Введите пароль-фразу.");
      } else {
        setBioNotice(
          `Отпечаток не распознан. Осталось ${left} ${left === 1 ? "попытка" : "попытки"}.`,
        );
      }
      return;
    }

    setBioStopped(true);
    setBioNotice(
      result.code === "biometric_lockout"
        ? "Биометрия временно заблокирована. Введите пароль-фразу."
        : result.code === "biometric_key_missing"
          ? "Сохранённый ключ биометрии не найден. Введите пароль-фразу и включите биометрию заново."
          : result.code === "biometric_cancelled"
            ? "Вход по отпечатку отменён. Введите пароль-фразу."
            : result.message
              ? `Биометрия не сработала: ${result.message}`
              : "Биометрия недоступна. Введите пароль-фразу.",
    );
  }, [bioAttemptsLeft, bioBusy, bioStopped, unlockBiometric]);

  useEffect(() => {
    if (autoTried.current || !bioEnabled || bioAvailable !== true || bioStopped) {
      return;
    }
    const timer = setTimeout(() => {
      autoTried.current = true;
      void attemptBiometric();
    }, 400);
    return () => clearTimeout(timer);
  }, [attemptBiometric, bioAvailable, bioEnabled, bioStopped]);

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
      const path = await getStorage().pickFile("Выберите ключ-файл");
      if (!path) return;
      const bytes = await getStorage().readFile(path);
      setKeyfileBytes(bytes);
      setKeyfileName(getStorage().displayName(path));
    } catch {
      setLocalError("Не удалось прочитать ключ-файл.");
    } finally {
      setReadingKeyfile(false);
    }
  };

  const submit = async () => {
    setLocalError(null);
    let secret: UnlockSecret;

    if (effectiveMethod === "passphrase" || effectiveMethod === "passphrase-keyfile") {
      if (!passphrase) {
        setLocalError("Введите пароль-фразу.");
        return;
      }
    }
    if (effectiveMethod === "keyfile" || effectiveMethod === "passphrase-keyfile") {
      if (!keyfileBytes) {
        setLocalError("Выберите ключ-файл.");
        return;
      }
    }

    if (effectiveMethod === "passphrase") {
      secret = { kind: "passphrase", value: passphrase };
    } else if (effectiveMethod === "keyfile") {
      secret = {
        kind: "keyfile",
        bytesB64: toBase64(keyfileBytes!),
        passphrase: keyfilePassphrase || undefined,
      };
    } else {
      secret = {
        kind: "passphraseKeyfile",
        value: passphrase,
        bytesB64: toBase64(keyfileBytes!),
        passphrase: keyfilePassphrase || undefined,
      };
    }

    await unlock(secret);
  };

  const message = localError
    ? localError
    : error
      ? errorText(error)
      : null;

  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-[var(--bg)] px-4 pb-8 pt-[calc(2rem+env(safe-area-inset-top))] text-[var(--text)]">
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
              {publicInfo && vaultPath ? getStorage().displayName(vaultPath) : "Выберите хранилище"}
            </p>
            {vaultPath ? (
              <p className="mx-auto mt-1 max-w-[330px] truncate text-[10px] text-[var(--muted)]/65">
                {vaultPath.startsWith("pms-android-") ? "Папка Android (SAF)" : vaultPath}
              </p>
            ) : null}
          </div>

          <div className="px-6 py-5">
            {bioEnabled && bioAvailable === true && !bioStopped ? (
              <div className="mb-4">
                <Button
                  variant="secondary"
                  className="w-full"
                  onClick={() => void attemptBiometric()}
                  disabled={bioBusy || busy}
                >
                  {bioBusy ? (
                    <Loader2 size={15} className="animate-spin" />
                  ) : (
                    <Fingerprint size={16} />
                  )}
                  {bioBusy
                    ? "Проверяем отпечаток…"
                    : bioNotice
                      ? "Повторить"
                      : "Войти по отпечатку"}
                </Button>
                {bioNotice ? (
                  <p
                    role="status"
                    className="mt-2 text-center text-[10.5px] text-[var(--warning)]"
                  >
                    {bioNotice}
                  </p>
                ) : null}
              </div>
            ) : bioNotice ? (
              <p
                role="status"
                className="mb-4 text-center text-[10.5px] leading-relaxed text-[var(--muted)]"
              >
                {bioNotice}
              </p>
            ) : null}
            {slotKinds.length > 1 ? (
              <div
                className="mb-5 grid gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1"
                style={{ gridTemplateColumns: `repeat(${slotKinds.length}, minmax(0, 1fr))` }}
              >
                {slotKinds.map((kind) => (
                  <button
                    key={kind}
                    type="button"
                    onClick={() => chooseMethod(kind)}
                    className={cn(
                      "flex h-8 min-w-0 items-center justify-center gap-1.5 rounded-[6px] px-1.5 text-[11.5px] transition-colors",
                      effectiveMethod === kind
                        ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                        : "text-[var(--muted)] hover:text-[var(--text-soft)]",
                    )}
                  >
                    {kind === "passphrase" ? (
                      <KeyRound size={13} />
                    ) : kind === "keyfile" ? (
                      <FileKey size={13} />
                    ) : (
                      <ShieldCheck size={13} />
                    )}
                    {kind === "passphrase"
                      ? "Пароль-фраза"
                      : kind === "keyfile"
                        ? "Ключ-файл"
                        : "Пароль + файл"}
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
              {effectiveMethod !== "keyfile" ? (
                <Field
                  label="Пароль-фраза"
                  htmlFor="unlock-passphrase"
                  hint={
                    effectiveMethod === "passphrase-keyfile"
                      ? "Вместе с ключ-файлом. Нужны оба."
                      : undefined
                  }
                >
                  <SecretInput
                    id="unlock-passphrase"
                    value={passphrase}
                    onChange={(event) => {
                      setPassphrase(event.target.value);
                      setLocalError(null);
                      clearError();
                    }}
                    placeholder="Введите пароль-фразу"
                  />
                </Field>
              ) : null}

              {effectiveMethod !== "passphrase" ? (
                <div className={cn("flex flex-col gap-3", effectiveMethod === "passphrase-keyfile" && "mt-4")}>
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
              ) : null}

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
