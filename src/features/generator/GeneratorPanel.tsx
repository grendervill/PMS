import { History, Loader2, RefreshCw, Undo2, Wand2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { Input } from "@/components/ui/Input";
import { Toggle } from "@/components/ui/Toggle";
import {
  clearGeneratorHistory,
  pushGeneratorHistory,
  readGeneratorHistory,
  summarizeHistoryValue,
  type GeneratorHistoryEntry,
} from "@/features/generator/history";
import {
  clampMinimums,
  clampWords,
  DEFAULT_PASSPHRASE_OPTIONS,
  defaultLoginOptions,
  defaultPasswordOptions,
  hasSelectedCharset,
  LOGIN_LENGTH_PRESETS,
  LOGIN_MAX_LENGTH,
  MAX_EXCLUDE_CHARS,
  normalizeExcludeChars,
  PASSPHRASE_MAX_WORDS,
  PASSPHRASE_MIN_WORDS,
  PASSPHRASE_SEPARATORS,
  PASSWORD_LENGTH_PRESETS,
  PASSWORD_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
  type EntropyInput,
  type PassphraseOptions,
  type PasswordOptions,
} from "@/features/generator/types";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import { vaultApi } from "@/features/vault/api";
import { cn } from "@/lib/cn";

export type GeneratorMode = "password" | "login" | "passphrase";

const GENERATE_ERROR = "Не удалось сгенерировать значение";

/**
 * The backend refuses a setup where the exclusion list empties a whole class.
 * Say so instead of showing the generic failure message.
 */
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
}: {
  value: string;
  mode: GeneratorMode;
  separator: string;
}) {
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
}: {
  mode: GeneratorMode;
  onUse?: (value: string) => void;
}) {
  const isPassword = mode === "password";
  const isPassphrase = mode === "passphrase";
  const [options, setOptions] = useState<PasswordOptions>(
    isPassword ? { ...defaultPasswordOptions } : { ...defaultLoginOptions },
  );
  const [passphrase, setPassphrase] = useState<PassphraseOptions>({
    ...DEFAULT_PASSPHRASE_OPTIONS,
  });
  const [generated, setGenerated] = useState("");
  const [strength, setStrength] = useState<PasswordStrength | null>(null);
  const [entropy, setEntropy] = useState<number | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<GeneratorHistoryEntry[]>(() => readGeneratorHistory());
  const initialOptions = useRef(options);
  const generatedOnce = useRef(false);

  /** Every successful generation lands in the session history. */
  const remember = (value: string) => {
    if (!value) return;
    setHistory(pushGeneratorHistory({ value, mode }));
  };

  const applyLength = (length: number) => {
    const next = clampMinimums({ ...options, length });
    setOptions(next);
    setError(null);
    void generate(next);
  };

  const restoreFromHistory = (entry: GeneratorHistoryEntry) => {
    setGenerated(entry.value);
    setError(null);
    if (mode === "password") void passwordStrength(entry.value).then(setStrength);
    // Outside the entry editor there is nobody to hand the value to.
    if (onUse) onUse(entry.value);
  };

  const generate = async (withOptions = options, withPassphrase = passphrase) => {
    if (isPassphrase) {
      setGenerating(true);
      setError(null);
      try {
        const value = await vaultApi.generatePassphrase(withPassphrase);
        setGenerated(value);
        remember(value);
      } catch (error) {
        setError(describeGenerateError(error));
      } finally {
        setGenerating(false);
      }
      return;
    }

    if (!hasSelectedCharset(withOptions)) {
      setError("Выберите хотя бы один набор символов");
      setGenerated("");
      return;
    }

    setGenerating(true);
    setError(null);
    try {
      const value = isPassword
        ? await vaultApi.generatePassword(withOptions)
        : await vaultApi.generateLogin(withOptions);
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
          ? { mode: "password", options }
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
  }, [mode, options, passphrase, generated]);

  const update = <K extends keyof PasswordOptions>(key: K, value: PasswordOptions[K]) => {
    setOptions((current) => clampMinimums({ ...current, [key]: value }));
    setError(null);
  };

  const updatePassphrase = <K extends keyof PassphraseOptions>(
    key: K,
    value: PassphraseOptions[K],
  ) => {
    setPassphrase((current) => ({ ...current, [key]: value }));
    setError(null);
  };

  const maxLength = isPassword ? PASSWORD_MAX_LENGTH : LOGIN_MAX_LENGTH;
  const minLength = PASSWORD_MIN_LENGTH;
  const lengthPresets = isPassword ? PASSWORD_LENGTH_PRESETS : LOGIN_LENGTH_PRESETS;
  const score = strength?.score ?? 0;
  const color =
    score <= 1 ? "bg-[var(--danger)]" : score === 2 ? "bg-[var(--warning)]" : "bg-[var(--accent)]";

  return (
    <div className="grid gap-4">
      <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)] p-3.5">
        <div className="flex items-start gap-2">
          <div
            data-testid="generated-value"
            className="pms-mono min-h-12 min-w-0 flex-1 text-[21px] leading-snug text-[var(--text)]"
          >
            {generated ? (
              <GeneratedValue value={generated} mode={mode} separator={passphrase.separator} />
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

        {isPassword && generated ? (
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
      </div>

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

          <div className="grid gap-x-5">
            <Toggle
              label="Заглавные буквы"
              description="Каждое слово начнётся с заглавной буквы."
              checked={passphrase.capitalize}
              onCheckedChange={(checked) => updatePassphrase("capitalize", checked)}
            />
            <Toggle
              label="Добавить цифру"
              description="В конце фразы появится случайная цифра."
              checked={passphrase.includeNumber}
              onCheckedChange={(checked) => updatePassphrase("includeNumber", checked)}
            />
          </div>
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
              onCheckedChange={(checked) => update("lowercase", checked)}
            />
            <Toggle
              label="Заглавные"
              checked={options.uppercase}
              onCheckedChange={(checked) => update("uppercase", checked)}
            />
            <Toggle
              label="Цифры"
              checked={options.digits}
              onCheckedChange={(checked) => update("digits", checked)}
            />
            <Toggle
              label="Символы"
              checked={options.symbols}
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
        {onUse ? (
          <Button
            size="sm"
            onClick={() => {
              if (generated) onUse(generated);
            }}
            disabled={!generated || generating}
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
