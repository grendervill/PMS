import { useState } from "react";
import { GeneratorPanel, type GeneratorMode } from "@/features/generator/GeneratorPanel";
import { cn } from "@/lib/cn";

const MODES: { value: GeneratorMode; label: string; description: string }[] = [
  {
    value: "password",
    label: "Пароль",
    description:
      "Настройте длину и наборы символов. Каждое нажатие «Сгенерировать» создаёт новое значение с текущими правилами.",
  },
  {
    value: "login",
    label: "Логин",
    description:
      "Короткое значение для логина: строчные, цифры и при желании символы. Пересоздавайте, пока результат не устроит.",
  },
  {
    value: "passphrase",
    label: "Парольная фраза",
    description: "Слова из списка проще запомнить и набрать. Длина фразы задаётся числом слов.",
  },
];

export function GeneratorView() {
  const [mode, setMode] = useState<GeneratorMode>("password");
  const active = MODES.find((item) => item.value === mode);

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-xl">
        <div className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-5 sm:p-6">
          <div className="mb-5">
            <h2 className="pms-display text-[26px] leading-tight">Создать секрет</h2>
            <p className="mt-1.5 text-[12px] leading-relaxed text-[var(--muted)]">
              {active?.description ?? ""}
            </p>
          </div>

          <div className="mb-5 grid grid-cols-3 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
            {MODES.map((item) => (
              <button
                key={item.value}
                type="button"
                onClick={() => setMode(item.value)}
                className={cn(
                  "min-h-8 rounded-[6px] px-1 py-1 text-[11.5px] leading-tight transition-colors",
                  mode === item.value
                    ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                    : "text-[var(--muted)] hover:text-[var(--text-soft)]",
                )}
              >
                {item.label}
              </button>
            ))}
          </div>

          <GeneratorPanel key={mode} mode={mode} />
        </div>
      </div>
    </div>
  );
}
