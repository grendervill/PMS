import {
  History,
  Loader2,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Undo2,
  Upload,
  Wand2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { Input } from "@/components/ui/Input";
import { Toggle } from "@/components/ui/Toggle";
import { useToast } from "@/components/ui/toast-context";
import {
  clearGeneratorHistory,
  pushGeneratorHistory,
  readGeneratorHistory,
  summarizeHistoryValue,
  type GeneratorHistoryEntry,
} from "@/features/generator/history";
import {
  clampMinimums,
  clampPinLength,
  clampPronounceableLength,
  clampWords,
  DEFAULT_PASSPHRASE_OPTIONS,
  DEFAULT_PIN_OPTIONS,
  DEFAULT_PRONOUNCEABLE_OPTIONS,
  defaultLoginOptions,
  defaultPasswordOptions,
  hasSelectedCharset,
  LOGIN_LENGTH_PRESETS,
  LOGIN_MAX_LENGTH,
  MAX_EXCLUDE_CHARS,
  normalizeExcludeChars,
  NUMBER_POSITIONS,
  PASSPHRASE_MAX_WORDS,
  PASSPHRASE_MIN_WORDS,
  PASSPHRASE_SEPARATORS,
  PASSWORD_KINDS,
  PASSWORD_LENGTH_PRESETS,
  PASSWORD_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
  PIN_MAX_LENGTH,
  PIN_MIN_LENGTH,
  PRONOUNCEABLE_MAX_LENGTH,
  PRONOUNCEABLE_MIN_LENGTH,
  type EntropyInput,
  type NumberPosition,
  type PassphraseOptions,
  type PasswordKind,
  type PasswordOptions,
  type PinOptions,
  type PronounceableOptions,
  type WordlistInfo,
  type WordlistSource,
} from "@/features/generator/types";
import { optionsWithPolicy, policyLengthBounds, policyNote } from "@/features/generator/policy";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import { vaultApi } from "@/features/vault/api";
import type { PasswordPolicy } from "@/features/vault/types";
import { getStorage } from "@/lib/storage";
import { cn } from "@/lib/cn";

export type GeneratorMode = "password" | "login" | "passphrase";

/** Everything needed to (re)generate, so a handler can act on fresh state. */
type GenerateParams = {
  kind: PasswordKind;
  password: PasswordOptions;
  passphrase: PassphraseOptions;
  pin: PinOptions;
  pronounceable: PronounceableOptions;
};

const GENERATE_ERROR = "Не удалось сгенерировать значение";

/**
 * The backend refuses a setup where the exclusion list empties a whole class.
 * Say so instead of showing the generic failure message.
 */
/** Message for a backend error, preferring the specific wordlist codes. */
function errorMessage(error: unknown): string {
  const code =
    error && typeof error === "object" && "code" in error
      ? (error as { code?: unknown }).code
      : null;
  if (code === "wordlist_too_small") {
    return "В словнике слишком мало слов: нужно не меньше 512";
  }
  if (code === "wordlist_too_large") {
    return "В словнике слишком много слов: максимум 50 000";
  }
  if (code === "wordlist_invalid") {
    return "Словник должен содержать по одному слову в строке (буквы, дефис, апостроф; 2–24 символа)";
  }
  return error && typeof error === "object" && "message" in error
    ? String((error as { message: unknown }).message)
    : GENERATE_ERROR;
}

function describeGenerateError(error: unknown): string {
  // Duck-typed on purpose: tests replace the whole `vault/api` module.
  const code =
    error && typeof error === "object" && "code" in error
      ? (error as { code?: unknown }).code
      : null;

  if (code === "empty_charset") {
    return "Список исключений убирает целый набор символов — уберите часть или включите другой набор";
  }
  if (code === "bad_params") {
    return "Проверьте параметры генерации";
  }
  return GENERATE_ERROR;
}

/**
 * Passphrases are long and hyphenated; `break-all` cut words in half. Breaking
 * only after the separator keeps every word readable.
 */
function GeneratedValue({
  value,
  mode,
  separator,
  syllables = false,
}: {
  value: string;
  mode: GeneratorMode;
  separator: string;
  /** Readable values break between syllables instead of mid-syllable. */
  syllables?: boolean;
}) {
  if (syllables) {
    return (
      <span className="break-words">
        {Array.from(value).map((character, index) => (
          <span key={`${character}-${index}`}>
            {index > 0 && index % 2 === 0 ? <wbr /> : null}
            {character}
          </span>
        ))}
      </span>
    );
  }
  if (mode !== "passphrase" || !separator || !value.includes(separator)) {
    return <span className="break-all">{value}</span>;
  }
  const parts = value.split(separator);
  return (
    <span className="break-words">
      {parts.map((part, index) => (
        <span key={`${part}-${index}`}>
          {part}
          {index < parts.length - 1 ? (
            <>
              {separator}
              <wbr />
            </>
          ) : null}
        </span>
      ))}
    </span>
  );
}

export function GeneratorPanel({
  mode,
  onUse,
  policy,
}: {
  mode: GeneratorMode;
  onUse?: (value: string) => void;
  /** The site's password rules, when the generator is opened from an entry. */
  policy?: PasswordPolicy | null;
}) {
  const isPassword = mode === "password";
  const isPassphrase = mode === "passphrase";
  // Rules describe a password, not a login: the login generator stays manual.
  const activePolicy = isPassword ? (policy ?? null) : null;
  const [options, setOptions] = useState<PasswordOptions>(() =>
    optionsWithPolicy(
      isPassword ? { ...defaultPasswordOptions } : { ...defaultLoginOptions },
      isPassword ? (policy ?? null) : null,
    ),
  );
  const [passphrase, setPassphrase] = useState<PassphraseOptions>({
    ...DEFAULT_PASSPHRASE_OPTIONS,
  });
  const [kind, setKind] = useState<PasswordKind>("random");
  const [pin, setPin] = useState<PinOptions>({ ...DEFAULT_PIN_OPTIONS });
  const [pronounceable, setPronounceable] = useState<PronounceableOptions>({
    ...DEFAULT_PRONOUNCEABLE_OPTIONS,
  });
  const [generated, setGenerated] = useState("");
  const [strength, setStrength] = useState<PasswordStrength | null>(null);
  const [entropy, setEntropy] = useState<number | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [breach, setBreach] = useState<{ value: string; count: number } | null>(null);
  const [breachBusy, setBreachBusy] = useState(false);
  const [history, setHistory] = useState<GeneratorHistoryEntry[]>(() => readGeneratorHistory());
  const [wordlist, setWordlist] = useState<WordlistInfo | null>(null);
  const [wordlistBusy, setWordlistBusy] = useState(false);
  const toast = useToast();
  useEffect(() => {
    if (mode !== "passphrase") return;
    let active = true;
    void vaultApi
      .generatorWordlist()
      .then((info) => {
        if (active) setWordlist(info);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [mode]);

  const initialOptions = useRef(options);
  const generatedOnce = useRef(false);

  /** Every successful generation lands in the session history. */
  const remember = (value: string) => {
    if (!value) return;
    setHistory(pushGeneratorHistory({ value, mode }));
  };

  const applyLength = (length: number) => {
    const next = clampMinimums(optionsWithPolicy({ ...options, length }, activePolicy));
    setOptions(next);
    setError(null);
    void generate({ password: next });
  };

  const restoreFromHistory = (entry: GeneratorHistoryEntry) => {
    setGenerated(entry.value);
    setError(null);
    if (mode === "password") void passwordStrength(entry.value).then(setStrength);
    // Outside the entry editor there is nobody to hand the value to.
    if (onUse) onUse(entry.value);
  };

  const generate = async (overrides: Partial<GenerateParams> = {}) => {
    const params: GenerateParams = {
      kind,
      password: options,
      passphrase,
      pin,
      pronounceable,
      ...overrides,
    };

    if (isPassphrase) {
      setGenerating(true);
      setError(null);
      try {
        const value = await vaultApi.generatePassphrase(params.passphrase);
        setGenerated(value);
        remember(value);
      } catch (error) {
        setError(describeGenerateError(error));
      } finally {
        setGenerating(false);
      }
      return;
    }

    if (isPassword && params.kind === "pin") {
      setGenerating(true);
      setError(null);
      try {
        const value = await vaultApi.generatePin(params.pin);
        setGenerated(value);
        remember(value);
      } catch (error) {
        setError(describeGenerateError(error));
      } finally {
        setGenerating(false);
      }
      return;
    }

    // The readable charset is fixed; only the random password and the login
    // depend on the selected classes.
    const needsCharset = !isPassword || params.kind === "random";
    if (needsCharset && !hasSelectedCharset(params.password)) {
      setError("Выберите хотя бы один набор символов");
      setGenerated("");
      return;
    }

    setGenerating(true);
    setError(null);
    try {
      const value =
        isPassword && params.kind === "readable"
          ? await vaultApi.generatePronounceable(params.pronounceable)
          : isPassword
            ? await vaultApi.generatePassword(params.password)
            : await vaultApi.generateLogin(params.password);
      setGenerated(value);
      remember(value);
      if (isPassword) {
        setStrength(await passwordStrength(value));
      }
    } catch (error) {
      setError(describeGenerateError(error));
    } finally {
      setGenerating(false);
    }
  };

  useEffect(() => {
    if (generatedOnce.current) return;
    generatedOnce.current = true;
    void (async () => {
      setGenerating(true);
      try {
        const value =
          mode === "passphrase"
            ? await vaultApi.generatePassphrase({ ...DEFAULT_PASSPHRASE_OPTIONS })
            : mode === "password"
              ? await vaultApi.generatePassword(initialOptions.current)
              : await vaultApi.generateLogin(initialOptions.current);
        setGenerated(value);
        // Inlined instead of `remember()` so the effect keeps its `[mode]` deps.
        setHistory(pushGeneratorHistory({ value, mode }));
        if (mode === "password") setStrength(await passwordStrength(value));
      } catch (error) {
        setError(describeGenerateError(error));
      } finally {
        setGenerating(false);
      }
    })();
  }, [mode]);

  useEffect(() => {
    const input: EntropyInput =
      mode === "passphrase"
        ? { mode: "passphrase", options: passphrase }
        : mode === "password"
          ? kind === "pin"
            ? { mode: "pin", options: pin }
            : kind === "readable"
              ? { mode: "pronounceable", options: pronounceable }
              : { mode: "password", options }
          : { mode: "login", options };

    let cancelled = false;
    void (async () => {
      try {
        const bits = await vaultApi.generatorEntropy(input);
        if (!cancelled) setEntropy(bits);
      } catch {
        // The estimate is cosmetic: a failed call must not break the generator.
        if (!cancelled) setEntropy(null);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [mode, kind, options, passphrase, pin, pronounceable, generated]);

  const update = <K extends keyof PasswordOptions>(key: K, value: PasswordOptions[K]) => {
    setOptions((current) =>
      clampMinimums(optionsWithPolicy({ ...current, [key]: value }, activePolicy)),
    );
    setError(null);
  };

  const updatePassphrase = <K extends keyof PassphraseOptions>(
    key: K,
    value: PassphraseOptions[K],
  ) => {
    const next = { ...passphrase, [key]: value };
    setPassphrase(next);
    setError(null);
    // Changing the word list changes the entropy base; regenerating keeps the
    // shown value and the shown number in agreement.
    if (key === "wordlist" && isPassphrase) void generate({ passphrase: next });
  };

  /** Switches between a random password, a readable one and a PIN. */
  const changeKind = (next: PasswordKind) => {
    if (next === kind) return;
    setKind(next);
    setStrength(null);
    setError(null);
    void generate({ kind: next });
  };

  /** k-anonymity check; the password itself never leaves the device. */
  const checkBreach = async () => {
    if (!generated) return;
    const value = generated;
    setBreachBusy(true);
    try {
      setBreach({ value, count: await vaultApi.breachCount(value) });
    } catch {
      toast.show({
        title: "Не удалось проверить утечки",
        description: "Проверьте подключение к сети и попробуйте снова.",
        tone: "error",
      });
    } finally {
      setBreachBusy(false);
    }
  };

  /**
   * Imports a word list chosen through the native dialog. The file is copied
   * into the app data directory and validated there (>= 512 words, one per
   * line), so the passphrase can use any language.
   */
  const importWordlist = async () => {
    setWordlistBusy(true);
    try {
      const path = await getStorage().pickFile("Выберите словник (.txt)");
      if (!path) return;
      const info = await vaultApi.importWordlist(path);
      setWordlist(info);
      updatePassphrase("wordlist", "custom");
    } catch (importError) {
      setError(errorMessage(importError));
    } finally {
      setWordlistBusy(false);
    }
  };

  const removeWordlist = async () => {
    setWordlistBusy(true);
    try {
      const info = await vaultApi.clearWordlist();
      setWordlist(info);
      if (passphrase.wordlist === "custom") updatePassphrase("wordlist", "eff");
    } catch (importError) {
      setError(errorMessage(importError));
    } finally {
      setWordlistBusy(false);
    }
  };

  const policyBounds = policyLengthBounds(activePolicy);
  const maxLength = activePolicy
    ? Math.min(policyBounds.max, isPassword ? PASSWORD_MAX_LENGTH : LOGIN_MAX_LENGTH)
    : isPassword
      ? PASSWORD_MAX_LENGTH
      : LOGIN_MAX_LENGTH;
  const minLength = activePolicy ? Math.min(policyBounds.min, maxLength) : PASSWORD_MIN_LENGTH;
  const lengthPresets = (isPassword ? PASSWORD_LENGTH_PRESETS : LOGIN_LENGTH_PRESETS).filter(
    (preset) => preset >= minLength && preset <= maxLength,
  );
  const note = policyNote(activePolicy);
  const score = strength?.score ?? 0;
  const color =
    score <= 1 ? "bg-[var(--danger)]" : score === 2 ? "bg-[var(--warning)]" : "bg-[var(--accent)]";
  const isLogin = mode === "login";
  /**
   * A random password or a readable value that zxcvbn rates 0-1 is not worth
   * saving: the "Использовать" button is disabled while copying still works.
   * A PIN is weak by nature, so it gets a warning instead of a block.
   */
  const tooWeak =
    isPassword && kind !== "pin" && Boolean(generated) && strength !== null && strength.score <= 1;
  // A result for a previous value must not describe the current one.
  const breachCount = breach && breach.value === generated ? breach.count : null;

  return (
    <div className="grid gap-4">
      {note ? (
        <p
          data-testid="generator-policy-note"
          className="flex items-start gap-1.5 rounded-[var(--radius-sm)] border border-[var(--accent)]/25 bg-[var(--accent)]/6 px-2.5 py-2 text-[11px] leading-relaxed text-[var(--text-soft)]"
        >
          <ShieldCheck size={12} className="mt-0.5 shrink-0 text-[var(--accent-soft)]" />
          <span className="min-w-0">{note}</span>
        </p>
      ) : null}
      <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)] p-3.5">
        <div className="flex items-start gap-2">
          <div
            data-testid="generated-value"
            className="pms-mono min-h-12 min-w-0 flex-1 text-[21px] leading-snug text-[var(--text)]"
          >
            {generated ? (
              <GeneratedValue
                value={generated}
                mode={mode}
                separator={passphrase.separator}
                syllables={isPassword && kind === "readable"}
              />
            ) : (
              <span className="text-[var(--muted)]">Нажмите «Сгенерировать»</span>
            )}
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void generate()}
            disabled={generating}
            aria-label="Сгенерировать заново"
            title="Сгенерировать заново"
          >
            {generating ? <Loader2 size={15} className="animate-spin" /> : <RefreshCw size={15} />}
          </Button>
        </div>

        {isPassword && kind !== "pin" && generated ? (
          <div className="mt-3 flex items-center gap-2.5">
            <div className="flex flex-1 gap-1">
              {[0, 1, 2, 3].map((index) => (
                <span
                  key={index}
                  className={cn(
                    "h-1 flex-1 rounded-full transition-colors",
                    index < score ? color : "bg-[var(--surface-3)]",
                  )}
                />
              ))}
            </div>
            <span className="w-24 text-right text-[10.5px] text-[var(--muted)]">
              {strength?.label ?? "Проверяем…"}
            </span>
          </div>
        ) : null}

        {entropy !== null ? (
          <p className="mt-2 text-[10.5px] text-[var(--muted)]">~{Math.round(entropy)} бит</p>
        ) : null}

        {generated && !isLogin ? (
          <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void checkBreach()}
              disabled={breachBusy}
              className="px-1.5 text-[10.5px] text-[var(--muted)]"
            >
              {breachBusy ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <ShieldAlert size={12} />
              )}
              Проверить утечки
            </Button>
            {breachCount === null ? null : breachCount > 0 ? (
              <span role="status" className="text-[10.5px] text-[var(--danger)]">
                Найден в {breachCount.toLocaleString("ru-RU")} утечках — не используйте
              </span>
            ) : (
              <span
                role="status"
                className="flex items-center gap-1 text-[10.5px] text-[var(--accent)]"
              >
                <ShieldCheck size={12} />В известных утечках не найден
              </span>
            )}
          </div>
        ) : null}
      </div>

      {isPassword ? (
        <div
          role="group"
          aria-label="Тип пароля"
          className="grid grid-cols-3 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1"
        >
          {PASSWORD_KINDS.map((item) => (
            <button
              key={item.value}
              type="button"
              onClick={() => changeKind(item.value)}
              aria-pressed={kind === item.value}
              className={cn(
                "min-h-8 rounded-[6px] px-1 py-1 text-[11.5px] leading-tight transition-colors",
                kind === item.value
                  ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                  : "text-[var(--muted)] hover:text-[var(--text-soft)]",
              )}
            >
              {item.label}
            </button>
          ))}
        </div>
      ) : null}

      {isPassphrase ? (
        <div className="grid gap-4">
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[12px] text-[var(--text-soft)]">Слов в фразе</span>
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={PASSPHRASE_MIN_WORDS}
                  max={PASSPHRASE_MAX_WORDS}
                  value={passphrase.words}
                  onChange={(event) =>
                    updatePassphrase("words", clampWords(Number(event.target.value)))
                  }
                  className="h-8 w-16 px-2 text-center text-[12px]"
                  aria-label="Количество слов"
                />
                <span className="text-[10.5px] text-[var(--muted)]">слов</span>
              </div>
            </div>
            <input
              type="range"
              min={PASSPHRASE_MIN_WORDS}
              max={PASSPHRASE_MAX_WORDS}
              value={passphrase.words}
              onChange={(event) => updatePassphrase("words", Number(event.target.value))}
              className="h-1 w-full accent-[var(--accent)]"
              aria-label="Количество слов ползунком"
            />
          </div>

          <label className="grid gap-1.5 text-[12px] text-[var(--text-soft)]">
            Разделитель
            <select
              aria-label="Разделитель"
              value={passphrase.separator}
              onChange={(event) => updatePassphrase("separator", event.target.value)}
              className="h-9 w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-2.5 text-[12px] text-[var(--text)] outline-none focus:border-[var(--accent)]"
            >
              {PASSPHRASE_SEPARATORS.map((option) => (
                <option key={option.label} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <div className="grid gap-1.5 text-[12px] text-[var(--text-soft)]">
            <span>Словник</span>
            <div className="flex items-center gap-2">
              <select
                aria-label="Словник"
                value={passphrase.wordlist}
                onChange={(event) =>
                  updatePassphrase("wordlist", event.target.value as WordlistSource)
                }
                className="h-9 min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-2.5 text-[12px] text-[var(--text)] outline-none focus:border-[var(--accent)]"
              >
                <option value="eff">EFF, английский ({wordlist?.builtinWords ?? 7776} слов)</option>
                <option value="custom" disabled={!wordlist?.customWords}>
                  {wordlist?.customWords
                    ? `Свой, ${wordlist.customWords} слов`
                    : "Свой (не импортирован)"}
                </option>
              </select>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void importWordlist()}
                disabled={wordlistBusy}
              >
                {wordlistBusy ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <Upload size={13} />
                )}
                Импорт
              </Button>
              {wordlist?.customWords ? (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void removeWordlist()}
                  disabled={wordlistBusy}
                  aria-label="Удалить свой словник"
                >
                  <Trash2 size={13} />
                </Button>
              ) : null}
            </div>
            <p className="text-[10px] leading-relaxed text-[var(--muted)]">
              Текстовый файл: по одному слову в строке, от {wordlist?.minCustomWords ?? 512} слов.
              Русские фразы запоминаются лучше английских.
            </p>
          </div>

          <div className="grid gap-x-5">
            <Toggle
              label="Заглавные буквы"
              description="Каждое слово начнётся с заглавной буквы."
              checked={passphrase.capitalize}
              onCheckedChange={(checked) => updatePassphrase("capitalize", checked)}
            />
            <Toggle
              label="Добавить цифру"
              description={
                passphrase.numberPosition === "start"
                  ? "В начале фразы появится случайная цифра."
                  : "В конце фразы появится случайная цифра."
              }
              checked={passphrase.includeNumber}
              onCheckedChange={(checked) => updatePassphrase("includeNumber", checked)}
            />
            {passphrase.includeNumber ? (
              <label className="grid gap-1.5 pt-1 text-[11px] text-[var(--text-soft)]">
                Где цифра
                <select
                  aria-label="Где цифра"
                  value={passphrase.numberPosition}
                  onChange={(event) =>
                    updatePassphrase("numberPosition", event.target.value as NumberPosition)
                  }
                  className="h-8 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] px-2 text-[11.5px] text-[var(--text)] outline-none focus:border-[var(--accent)]"
                >
                  {NUMBER_POSITIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
            ) : null}
          </div>
        </div>
      ) : isPassword && kind === "pin" ? (
        <div className="grid gap-4">
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[12px] text-[var(--text-soft)]">Длина</span>
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={PIN_MIN_LENGTH}
                  max={PIN_MAX_LENGTH}
                  value={pin.length}
                  onChange={(event) =>
                    setPin((current) => ({
                      ...current,
                      length: clampPinLength(Number(event.target.value)),
                    }))
                  }
                  className="h-8 w-16 px-2 text-center text-[12px]"
                  aria-label="Длина ПИН-кода"
                />
                <span className="text-[10.5px] text-[var(--muted)]">цифр</span>
              </div>
            </div>
            <input
              type="range"
              min={PIN_MIN_LENGTH}
              max={PIN_MAX_LENGTH}
              value={pin.length}
              onChange={(event) =>
                setPin((current) => ({ ...current, length: Number(event.target.value) }))
              }
              className="h-1 w-full accent-[var(--accent)]"
              aria-label="Длина ПИН-кода ползунком"
            />
          </div>

          <Toggle
            label="Исключать 1111 и 1234"
            description="Не предлагать одинаковые цифры и простые последовательности."
            checked={pin.excludeRepeats}
            onCheckedChange={(checked) =>
              setPin((current) => ({ ...current, excludeRepeats: checked }))
            }
          />

          <p className="text-[10.5px] leading-relaxed text-[var(--warning)]">
            ПИН-код слабее пароля: он защищает только от подбора у экрана.
          </p>
        </div>
      ) : isPassword && kind === "readable" ? (
        <div className="grid gap-4">
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[12px] text-[var(--text-soft)]">Длина</span>
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={PRONOUNCEABLE_MIN_LENGTH}
                  max={PRONOUNCEABLE_MAX_LENGTH}
                  value={pronounceable.length}
                  onChange={(event) =>
                    setPronounceable((current) => ({
                      ...current,
                      length: clampPronounceableLength(Number(event.target.value)),
                    }))
                  }
                  className="h-8 w-16 px-2 text-center text-[12px]"
                  aria-label="Длина читаемого пароля"
                />
                <span className="text-[10.5px] text-[var(--muted)]">символов</span>
              </div>
            </div>
            <input
              type="range"
              min={PRONOUNCEABLE_MIN_LENGTH}
              max={PRONOUNCEABLE_MAX_LENGTH}
              value={pronounceable.length}
              onChange={(event) =>
                setPronounceable((current) => ({
                  ...current,
                  length: Number(event.target.value),
                }))
              }
              className="h-1 w-full accent-[var(--accent)]"
              aria-label="Длина читаемого пароля ползунком"
            />
          </div>

          <Toggle
            label="С заглавной буквы"
            description="Первая буква станет заглавной — так проще читать вслух."
            checked={pronounceable.capitalize}
            onCheckedChange={(checked) =>
              setPronounceable((current) => ({ ...current, capitalize: checked }))
            }
          />

          <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
            Слоги из согласных и гласных без похожих символов: значение легко продиктовать, но оно
            заметно слабее случайного пароля той же длины.
          </p>
        </div>
      ) : (
        <>
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[12px] text-[var(--text-soft)]">Длина</span>
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={minLength}
                  max={maxLength}
                  value={options.length}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    update(
                      "length",
                      Number.isFinite(value)
                        ? Math.max(minLength, Math.min(maxLength, value))
                        : minLength,
                    );
                  }}
                  className="h-8 w-16 px-2 text-center text-[12px]"
                  aria-label="Длина"
                />
                <span className="text-[10.5px] text-[var(--muted)]">символов</span>
              </div>
            </div>
            <div className="flex flex-wrap gap-1.5" role="group" aria-label="Быстрый выбор длины">
              {lengthPresets.map((preset) => {
                const active = options.length === preset;
                return (
                  <button
                    key={preset}
                    type="button"
                    onClick={() => applyLength(preset)}
                    aria-pressed={active}
                    aria-label={`Длина ${preset}`}
                    className={cn(
                      "h-7 min-w-8 rounded-[6px] border px-1.5 text-[11px] tabular-nums transition-colors",
                      active
                        ? "border-[var(--accent)] bg-[var(--accent)]/15 text-[var(--text)]"
                        : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--border-strong)] hover:text-[var(--text-soft)]",
                    )}
                  >
                    {preset}
                  </button>
                );
              })}
            </div>
            <input
              type="range"
              min={minLength}
              max={maxLength}
              value={options.length}
              onChange={(event) => update("length", Number(event.target.value))}
              className="h-1 w-full accent-[var(--accent)]"
              aria-label="Длина ползунком"
            />
          </div>

          <div className="grid grid-cols-2 gap-x-5">
            <Toggle
              label="Строчные"
              checked={options.lowercase}
              disabled={activePolicy?.requireLowercase ?? false}
              onCheckedChange={(checked) => update("lowercase", checked)}
            />
            <Toggle
              label="Заглавные"
              checked={options.uppercase}
              disabled={activePolicy?.requireUppercase ?? false}
              onCheckedChange={(checked) => update("uppercase", checked)}
            />
            <Toggle
              label="Цифры"
              checked={options.digits}
              disabled={activePolicy?.requireDigits ?? false}
              onCheckedChange={(checked) => update("digits", checked)}
            />
            <Toggle
              label="Символы"
              checked={options.symbols}
              disabled={activePolicy?.requireSymbols ?? false}
              onCheckedChange={(checked) => update("symbols", checked)}
            />
          </div>

          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <span
                className={cn(
                  "text-[12px]",
                  options.digits ? "text-[var(--text-soft)]" : "text-[var(--muted)]",
                )}
              >
                Минимум цифр
              </span>
              <Input
                type="number"
                min={0}
                max={options.length}
                value={options.minDigits}
                disabled={!options.digits}
                onChange={(event) => update("minDigits", Number(event.target.value) || 0)}
                className="h-8 w-16 px-2 text-center text-[12px]"
                aria-label="Минимум цифр"
              />
            </div>
            <div className="flex items-center justify-between">
              <span
                className={cn(
                  "text-[12px]",
                  options.symbols ? "text-[var(--text-soft)]" : "text-[var(--muted)]",
                )}
              >
                Минимум спецсимволов
              </span>
              <Input
                type="number"
                min={0}
                max={options.length}
                value={options.minSymbols}
                disabled={!options.symbols}
                onChange={(event) => update("minSymbols", Number(event.target.value) || 0)}
                className="h-8 w-16 px-2 text-center text-[12px]"
                aria-label="Минимум спецсимволов"
              />
            </div>
            <p className="text-[10.5px] leading-relaxed text-[var(--muted)]">
              Минимумы работают, только если включён соответствующий набор, и вместе не превышают
              длину.
            </p>
          </div>

          {options.symbols ? (
            <Input
              value={options.customSymbols ?? ""}
              onChange={(event) => update("customSymbols", event.target.value || null)}
              placeholder="Свои символы (необязательно): !@#$%"
              className="h-8 text-[12px]"
              aria-label="Свои символы"
            />
          ) : null}

          <Input
            value={options.excludeChars ?? ""}
            onChange={(event) => update("excludeChars", normalizeExcludeChars(event.target.value))}
            maxLength={MAX_EXCLUDE_CHARS}
            placeholder={"Исключить символы (необязательно): <>\\\"'`"}
            className="h-8 text-[12px]"
            aria-label="Исключить символы"
            title="Эти символы не попадут в результат. Каждый включённый набор должен остаться непустым."
          />

          <Toggle
            label="Исключить похожие"
            description="Убрать 0, O, 1, l, I и | — меньше путаницы при вводе."
            checked={options.excludeAmbiguous}
            onCheckedChange={(checked) => update("excludeAmbiguous", checked)}
          />
        </>
      )}

      {error ? (
        <p role="alert" className="text-[11.5px] text-[var(--danger)]">
          {error}
        </p>
      ) : null}

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-[var(--border)] pt-4">
        <Button variant="secondary" size="sm" onClick={() => void generate()} disabled={generating}>
          <Wand2 size={14} />
          Сгенерировать
        </Button>
        <CopyButton value={generated} label="Копировать сгенерированное" />
        {tooWeak ? (
          <p role="status" className="mr-auto text-[11px] text-[var(--danger)]">
            Слишком слабо — увеличьте длину
          </p>
        ) : null}
        {onUse ? (
          <Button
            size="sm"
            onClick={() => {
              if (generated) onUse(generated);
            }}
            disabled={!generated || generating || tooWeak}
          >
            Использовать
          </Button>
        ) : null}
      </div>

      {history.length > 0 ? (
        <div className="grid gap-2 border-t border-[var(--border)] pt-3.5">
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-1.5 text-[10.5px] uppercase tracking-wide text-[var(--muted)]">
              <History size={12} />
              История
            </span>
            <button
              type="button"
              onClick={() => {
                clearGeneratorHistory();
                setHistory([]);
              }}
              className="rounded-[6px] px-1.5 py-0.5 text-[10.5px] text-[var(--muted)] transition-colors hover:bg-[var(--surface-2)] hover:text-[var(--text-soft)]"
            >
              Очистить историю
            </button>
          </div>
          <ul data-testid="generator-history" className="grid gap-1">
            {history.map((entry, index) => (
              <li
                key={`${entry.at}-${index}-${entry.value.slice(0, 4)}`}
                className="flex items-center gap-1 rounded-[6px] bg-[var(--surface-2)] py-0.5 pl-2 pr-0.5"
              >
                <span
                  className="pms-mono min-w-0 flex-1 truncate text-[11.5px] text-[var(--text-soft)]"
                  title={entry.value}
                >
                  {summarizeHistoryValue(entry.value)}
                </span>
                <CopyButton value={entry.value} label={`Копировать значение ${index + 1}`} />
                <button
                  type="button"
                  onClick={() => restoreFromHistory(entry)}
                  aria-label={`Вернуть значение ${index + 1}`}
                  title="Подставить это значение снова"
                  className="inline-flex h-8 items-center gap-1 rounded-[var(--radius-sm)] px-1.5 text-[10.5px] text-[var(--muted)] transition-colors hover:bg-[var(--surface-3)] hover:text-[var(--text)]"
                >
                  <Undo2 size={12} />
                  Вернуть
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
