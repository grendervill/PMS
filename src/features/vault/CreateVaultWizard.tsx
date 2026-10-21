import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  FileKey,
  FolderOpen,
  Loader2,
  Lock,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { IconButton } from "@/components/ui/IconButton";
import { Input } from "@/components/ui/Input";
import { SecretInput } from "@/components/ui/SecretInput";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import { getStorage } from "@/lib/storage";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import {
  buildSecrets,
  defaultWizardDraft,
  hasErrors,
  validateWizard,
  type WizardDraft,
  type WizardErrorKey,
  type WizardErrors,
} from "@/features/vault/wizard";
import { cn } from "@/lib/cn";

const steps = ["Папка", "Защита", "Проверка"];

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "Хранилище";
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} Б`;
  return `${(bytes / 1024).toFixed(1)} КБ`;
}

export function CreateVaultWizard({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<WizardDraft>(defaultWizardDraft);
  const [errors, setErrors] = useState<WizardErrors>({});
  const [strength, setStrength] = useState<PasswordStrength | null>(null);
  const [readingKeyfile, setReadingKeyfile] = useState(false);
  const busy = useVaultStore((state) => state.busy);
  const storeError = useVaultStore((state) => state.error);
  const createVault = useVaultStore((state) => state.createVault);
  const clearError = useVaultStore((state) => state.clearError);

  const stepErrorKeys = useMemo<WizardErrorKey[]>(() => {
    if (step === 0) return ["folder", "name"];
    if (step === 1) return ["passphrase", "confirm", "methods", "keyfile"];
    return ["folder", "name", "passphrase", "confirm", "methods", "keyfile"];
  }, [step]);

  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      if (!draft.passphrase) {
        if (!cancelled) setStrength(null);
        return;
      }
      void passwordStrength(draft.passphrase).then((result) => {
        if (!cancelled) setStrength(result);
      });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [draft.passphrase]);

  const update = <K extends keyof WizardDraft>(key: K, value: WizardDraft[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setErrors((current) => {
      const next = { ...current };
      delete next[key as WizardErrorKey];
      return next;
    });
    clearError();
  };

  const pickFolder = async () => {
    const path = await getStorage().pickFolder();
    if (!path) return;
    setDraft((current) => ({
      ...current,
      folder: path,
      name: current.name || basename(path),
    }));
    setErrors((current) => {
      const next = { ...current };
      delete next.folder;
      delete next.name;
      return next;
    });
  };

  const pickKeyfile = async () => {
    setReadingKeyfile(true);
    try {
      const path = await getStorage().pickFile("Выберите ключ-файл");
      if (!path) return;
      const bytes = await getStorage().readFile(path);
      update("keyfile", { name: basename(path), bytes, passphrase: "" });
    } finally {
      setReadingKeyfile(false);
    }
  };

  const validateStep = () => {
    const nextErrors = validateWizard(draft);
    const filtered = Object.fromEntries(
      Object.entries(nextErrors).filter(([key]) => stepErrorKeys.includes(key as WizardErrorKey)),
    ) as WizardErrors;
    setErrors((current) => {
      const merged = { ...current };
      for (const key of stepErrorKeys) delete merged[key];
      return { ...merged, ...filtered };
    });
    return !hasErrors(filtered);
  };

  const next = () => {
    if (!validateStep()) return;
    setStep((current) => Math.min(current + 1, steps.length - 1));
  };

  const back = () => {
    if (step === 0) {
      onClose();
    } else {
      setStep((current) => Math.max(current - 1, 0));
    }
  };

  const submit = async () => {
    const nextErrors = validateWizard(draft);
    setErrors(nextErrors);
    if (hasErrors(nextErrors)) {
      setStep(nextErrors.passphrase || nextErrors.confirm || nextErrors.methods ? 1 : 0);
      return;
    }

    const { primary, secondary } = buildSecrets(draft);
    await createVault({
      folder: draft.folder,
      name: draft.name.trim(),
      kdfProfile: draft.kdfProfile,
      cipher: draft.cipher,
      primarySecret: primary,
      secondarySecret: secondary,
    });
  };

  const strengthScore = strength?.score ?? 0;
  const strengthColor =
    strengthScore <= 1
      ? "bg-[var(--danger)]"
      : strengthScore === 2
        ? "bg-[var(--warning)]"
        : "bg-[var(--accent)]";

  return (
    <main className="min-h-screen bg-[var(--bg)] text-[var(--text)]">
      <header className="border-b border-[var(--border)] px-5 py-4 sm:px-8">
        <div className="mx-auto flex max-w-4xl items-center justify-between">
          <button
            type="button"
            onClick={back}
            disabled={busy}
            className="flex h-9 items-center gap-2 rounded-[var(--radius-sm)] px-2 text-[12.5px] text-[var(--muted)] transition-colors hover:bg-[var(--surface-2)] hover:text-[var(--text)] disabled:opacity-50"
          >
            <ArrowLeft size={15} />
            {step === 0 ? "Назад" : "К шагу назад"}
          </button>
          <div className="flex items-center gap-2 text-[12px] text-[var(--muted)]">
            <Lock size={14} className="text-[var(--accent-soft)]" />
            Новое хранилище
          </div>
        </div>
      </header>

      <div className="mx-auto w-full max-w-4xl px-5 py-8 sm:px-8 sm:py-10">
        <div className="mb-8 flex items-center gap-2 sm:gap-3">
          {steps.map((label, index) => (
            <div key={label} className="flex min-w-0 flex-1 items-center gap-2 sm:gap-3">
              <span
                className={cn(
                  "grid size-7 shrink-0 place-items-center rounded-full border text-[11px] font-medium transition-colors",
                  index < step
                    ? "border-[var(--accent)] bg-[var(--accent)] text-[#08110d]"
                    : index === step
                      ? "border-[var(--seal)] bg-[var(--seal)]/10 text-[var(--seal)]"
                      : "border-[var(--border)] text-[var(--muted)]",
                )}
              >
                {index < step ? <Check size={13} /> : index + 1}
              </span>
              <span
                className={cn(
                  "hidden text-[11.5px] sm:block",
                  index === step ? "text-[var(--text-soft)]" : "text-[var(--muted)]",
                )}
              >
                {label}
              </span>
              {index < steps.length - 1 ? (
                <span className="h-px flex-1 bg-[var(--border)]" />
              ) : null}
            </div>
          ))}
        </div>

        <div className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-5 sm:p-7">
          {step === 0 ? (
            <section className="pms-rise">
              <h1 className="pms-display text-[28px] leading-tight">Где будет жить сейф?</h1>
              <p className="mt-2 max-w-lg text-[13px] leading-relaxed text-[var(--muted)]">
                Выберите пустую папку или создайте новую. Внутри появятся зашифрованный
                файл, вложения и резервные копии.
              </p>

              <div className="mt-6 flex flex-col gap-5">
                <Field
                  label="Папка хранилища"
                  htmlFor="vault-folder"
                  error={errors.folder}
                  hint="Например, Documents/PMS или папка на внешнем диске."
                >
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <div
                      id="vault-folder"
                      className={cn(
                        "flex h-10 min-w-0 flex-1 items-center rounded-[var(--radius-sm)] border bg-[var(--surface-2)] px-3 text-[12.5px]",
                        errors.folder
                          ? "border-[var(--danger)]"
                          : "border-[var(--border)]",
                      )}
                    >
                      <span className={cn("truncate", draft.folder ? "text-[var(--text)]" : "text-[var(--muted)]")}>
                        {draft.folder || "Папка не выбрана"}
                      </span>
                    </div>
                    <Button variant="secondary" onClick={() => void pickFolder()}>
                      <FolderOpen size={15} />
                      Выбрать
                    </Button>
                  </div>
                </Field>

                <Field
                  label="Название хранилища"
                  htmlFor="vault-name"
                  error={errors.name}
                  hint="Видно только вам; можно поменять позже."
                >
                  <Input
                    id="vault-name"
                    value={draft.name}
                    onChange={(event) => update("name", event.target.value)}
                    placeholder="Личное"
                    autoFocus
                  />
                </Field>
              </div>
            </section>
          ) : null}

          {step === 1 ? (
            <section className="pms-rise">
              <h1 className="pms-display text-[28px] leading-tight">Чем закрыть хранилище?</h1>
              <p className="mt-2 max-w-lg text-[13px] leading-relaxed text-[var(--muted)]">
                Пароль-фраза, ключ-файл или оба сразу. Если выбрать оба, для входа
                понадобятся и пароль-фраза, и ключ-файл. Пароль-фраза не хранится;
                ключ-файл остаётся отдельным файлом на вашем устройстве.
              </p>

              <div className="mt-6 grid gap-5">
                <div className="grid gap-4 sm:grid-cols-2">
                  <Field
                    label="Пароль-фраза"
                    htmlFor="vault-passphrase"
                    error={errors.passphrase}
                    hint="Минимум 8 символов. Лучше — длинная фраза."
                  >
                    <SecretInput
                      id="vault-passphrase"
                      value={draft.passphrase}
                      onChange={(event) => update("passphrase", event.target.value)}
                      placeholder="Например: тихий лес и медный ключ"
                    />
                  </Field>
                  <Field
                    label="Повторите пароль-фразу"
                    htmlFor="vault-confirm"
                    error={errors.confirm}
                  >
                    <SecretInput
                      id="vault-confirm"
                      value={draft.confirm}
                      onChange={(event) => update("confirm", event.target.value)}
                      placeholder="Ещё раз"
                    />
                  </Field>
                </div>

                {draft.passphrase ? (
                  <div className="flex items-center gap-3">
                    <div className="flex flex-1 gap-1">
                      {[0, 1, 2, 3].map((index) => (
                        <span
                          key={index}
                          className={cn(
                            "h-1 flex-1 rounded-full transition-colors",
                            index < strengthScore ? strengthColor : "bg-[var(--surface-3)]",
                          )}
                        />
                      ))}
                    </div>
                    <span className="w-24 text-right text-[10.5px] text-[var(--muted)]">
                      {strength?.label ?? "Проверяем…"}
                    </span>
                  </div>
                ) : null}

                <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/50 p-3.5">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="text-[12.5px] font-medium text-[var(--text-soft)]">
                        Ключ-файл <span className="font-normal text-[var(--muted)]">(необязательно)</span>
                      </p>
                      <p className="mt-1 text-[11px] leading-relaxed text-[var(--muted)]">
                        SSH-ключ или любой файл. Если это зашифрованный SSH-ключ,
                        укажите его парольную фразу ниже.
                      </p>
                      <p className="mt-1 text-[10.5px] leading-relaxed text-[var(--warning)]">
                        Содержимое обычного ключ-файла должно остаться неизменным.
                        Пересохранение, синхронизация или редактирование файла
                        сделают вход невозможным.
                      </p>
                      {draft.passphrase ? (
                        <p className="mt-1 text-[10.5px] leading-relaxed text-[var(--accent-soft)]">
                          Вместе с пароль-фразой: для входа понадобятся оба фактора.
                          Если нужен отдельный SSH-ключ как альтернатива, создайте
                          хранилище только с ключ-файлом.
                        </p>
                      ) : null}
                    </div>
                    {draft.keyfile ? (
                      <IconButton
                        label="Убрать ключ-файл"
                        size="sm"
                        onClick={() => update("keyfile", null)}
                      >
                        <Trash2 size={14} />
                      </IconButton>
                    ) : null}
                  </div>

                  {draft.keyfile ? (
                    <div className="mt-3 flex flex-col gap-3">
                      <div className="flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface)] px-3 py-2">
                        <FileKey size={15} className="shrink-0 text-[var(--accent-soft)]" />
                        <span className="min-w-0 flex-1 truncate text-[12px] text-[var(--text-soft)]">
                          {draft.keyfile.name}
                        </span>
                        <span className="text-[10.5px] text-[var(--muted)]">
                          {formatBytes(draft.keyfile.bytes.length)}
                        </span>
                      </div>
                      <Field
                        label="Парольная фраза ключа"
                        htmlFor="keyfile-passphrase"
                        hint="Оставьте пустой, если у ключа её нет."
                        error={errors.keyfile}
                      >
                        <SecretInput
                          id="keyfile-passphrase"
                          value={draft.keyfile.passphrase}
                          onChange={(event) =>
                            update("keyfile", {
                              ...draft.keyfile!,
                              passphrase: event.target.value,
                            })
                          }
                          placeholder="Если есть"
                        />
                      </Field>
                    </div>
                  ) : (
                    <Button
                      variant="secondary"
                      size="sm"
                      className="mt-3"
                      onClick={() => void pickKeyfile()}
                      disabled={readingKeyfile}
                    >
                      {readingKeyfile ? (
                        <Loader2 size={14} className="animate-spin" />
                      ) : (
                        <FileKey size={14} />
                      )}
                      Выбрать ключ-файл
                    </Button>
                  )}
                </div>

                <div className="grid gap-4 sm:grid-cols-2">
                  <Field label="Профиль шифрования" htmlFor="kdf-profile" hint="Чем выше, тем медленнее подбор.">
                    <select
                      id="kdf-profile"
                      value={draft.kdfProfile}
                      onChange={(event) =>
                        update("kdfProfile", event.target.value as WizardDraft["kdfProfile"])
                      }
                      className="h-10 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
                    >
                      <option value="standard">Стандартный · 64 МиБ</option>
                      <option value="strong">Усиленный · 256 МиБ</option>
                      <option value="paranoid">Параноидальный · 512 МиБ</option>
                    </select>
                  </Field>
                  <Field label="Шифр данных" htmlFor="cipher" hint="Оба варианта проверены; выбирайте по вкусу.">
                    <select
                      id="cipher"
                      value={draft.cipher}
                      onChange={(event) =>
                        update("cipher", event.target.value as WizardDraft["cipher"])
                      }
                      className="h-10 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-3 text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
                    >
                      <option value="xchacha20-poly1305">XChaCha20-Poly1305</option>
                      <option value="aes-256-gcm">AES-256-GCM</option>
                    </select>
                  </Field>
                </div>

                {errors.methods ? (
                  <p role="alert" className="text-xs text-[var(--danger)]">
                    {errors.methods}
                  </p>
                ) : null}
              </div>
            </section>
          ) : null}

          {step === 2 ? (
            <section className="pms-rise">
              <h1 className="pms-display text-[28px] leading-tight">Проверьте перед созданием</h1>
              <p className="mt-2 text-[13px] text-[var(--muted)]">
                После создания хранилище сразу откроется. Это последний шаг.
              </p>

              <div className="mt-6 divide-y divide-[var(--border)] overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)]">
                {[
                  ["Папка", draft.folder],
                  ["Название", draft.name.trim()],
                  [
                    "Способы входа",
                    [
                      draft.passphrase ? "Пароль-фраза" : null,
                      draft.keyfile ? `Ключ-файл · ${draft.keyfile.name}` : null,
                    ]
                      .filter(Boolean)
                      .join(" + "),
                  ],
                  ["Профиль", draft.kdfProfile === "standard" ? "Стандартный" : draft.kdfProfile === "strong" ? "Усиленный" : "Параноидальный"],
                  ["Шифр", draft.cipher === "xchacha20-poly1305" ? "XChaCha20-Poly1305" : "AES-256-GCM"],
                ].map(([label, value]) => (
                  <div key={label} className="flex items-start gap-4 px-4 py-3">
                    <span className="w-28 shrink-0 text-[11.5px] text-[var(--muted)]">{label}</span>
                    <span className="min-w-0 break-all text-[12.5px] text-[var(--text-soft)]">
                      {value}
                    </span>
                  </div>
                ))}
              </div>

              <div className="mt-5 flex items-start gap-3 rounded-[var(--radius-md)] border border-[var(--warning)]/30 bg-[var(--warning)]/8 px-4 py-3">
                <ShieldCheck size={17} className="mt-0.5 shrink-0 text-[var(--warning)]" />
                <p className="text-[11.5px] leading-relaxed text-[var(--text-soft)]">
                  Восстановления нет. Если потерять пароль-фразу и ключ-файл, данные
                  останутся зашифрованными навсегда. Сохраните выбранный способ входа
                  в надёжном месте.
                </p>
              </div>
            </section>
          ) : null}

          {storeError ? (
            <p role="alert" className="mt-5 rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-xs text-[var(--danger)]">
              {errorText(storeError)}
            </p>
          ) : null}

          <div className="mt-7 flex items-center justify-between gap-3 border-t border-[var(--border)] pt-5">
            <Button variant="ghost" onClick={back} disabled={busy}>
              {step === 0 ? "Отменить" : "Назад"}
            </Button>
            {step < 2 ? (
              <Button onClick={next} disabled={busy}>
                Далее
                <ArrowRight size={15} />
              </Button>
            ) : (
              <Button onClick={() => void submit()} disabled={busy}>
                {busy ? <Loader2 size={15} className="animate-spin" /> : <Lock size={15} />}
                Создать хранилище
              </Button>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}
