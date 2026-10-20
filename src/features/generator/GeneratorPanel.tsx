import { Loader2, RefreshCw, Wand2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { Input } from "@/components/ui/Input";
import { Toggle } from "@/components/ui/Toggle";
import {
  defaultLoginOptions,
  defaultPasswordOptions,
  hasSelectedCharset,
  type PasswordOptions,
} from "@/features/generator/types";
import { passwordStrength, type PasswordStrength } from "@/features/generator/strength";
import { vaultApi } from "@/features/vault/api";
import { cn } from "@/lib/cn";

export function GeneratorPanel({
  mode,
  onUse,
}: {
  mode: "password" | "login";
  onUse?: (value: string) => void;
}) {
  const isPassword = mode === "password";
  const [options, setOptions] = useState<PasswordOptions>(
    isPassword ? { ...defaultPasswordOptions } : { ...defaultLoginOptions },
  );
  const [generated, setGenerated] = useState("");
  const [strength, setStrength] = useState<PasswordStrength | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialOptions = useRef(options);
  const generatedOnce = useRef(false);

  const generate = async (withOptions = options) => {
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
      if (isPassword) {
        setStrength(await passwordStrength(value));
      }
    } catch {
      setError("Не удалось сгенерировать значение");
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
        const value = isPassword
          ? await vaultApi.generatePassword(initialOptions.current)
          : await vaultApi.generateLogin(initialOptions.current);
        setGenerated(value);
        if (isPassword) setStrength(await passwordStrength(value));
      } catch {
        setError("Не удалось сгенерировать значение");
      } finally {
        setGenerating(false);
      }
    })();
  }, [isPassword]);

  const update = <K extends keyof PasswordOptions>(key: K, value: PasswordOptions[K]) => {
    setOptions((current) => ({ ...current, [key]: value }));
    setError(null);
  };

  const maxLength = isPassword ? 128 : 64;
  const score = strength?.score ?? 0;
  const color =
    score <= 1
      ? "bg-[var(--danger)]"
      : score === 2
        ? "bg-[var(--warning)]"
        : "bg-[var(--accent)]";

  return (
    <div className="grid gap-4">
      <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)] p-3.5">
        <div className="flex items-start gap-2">
          <div className="pms-mono min-h-12 min-w-0 flex-1 break-all text-[21px] leading-snug text-[var(--text)]">
            {generated || <span className="text-[var(--muted)]">Нажмите «Сгенерировать»</span>}
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void generate()}
            disabled={generating}
            aria-label="Сгенерировать заново"
            title="Сгенерировать заново"
          >
            {generating ? (
              <Loader2 size={15} className="animate-spin" />
            ) : (
              <RefreshCw size={15} />
            )}
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
      </div>

      <div className="grid gap-2">
        <div className="flex items-center justify-between">
          <span className="text-[12px] text-[var(--text-soft)]">Длина</span>
          <div className="flex items-center gap-2">
            <Input
              type="number"
              min={8}
              max={maxLength}
              value={options.length}
              onChange={(event) => {
                const value = Number(event.target.value);
                update("length", Number.isFinite(value) ? Math.max(8, Math.min(maxLength, value)) : 8);
              }}
              className="h-8 w-16 px-2 text-center text-[12px]"
              aria-label="Длина"
            />
            <span className="text-[10.5px] text-[var(--muted)]">символов</span>
          </div>
        </div>
        <input
          type="range"
          min={8}
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

      {options.symbols ? (
        <Input
          value={options.customSymbols ?? ""}
          onChange={(event) => update("customSymbols", event.target.value || null)}
          placeholder="Свои символы (необязательно): !@#$%"
          className="h-8 text-[12px]"
          aria-label="Свои символы"
        />
      ) : null}

      <Toggle
        label="Исключить похожие"
        description="Убрать 0, O, 1, l, I и | — меньше путаницы при вводе."
        checked={options.excludeAmbiguous}
        onCheckedChange={(checked) => update("excludeAmbiguous", checked)}
      />

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
    </div>
  );
}
