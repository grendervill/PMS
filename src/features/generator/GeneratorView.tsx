import { useState } from "react";
import { GeneratorPanel } from "@/features/generator/GeneratorPanel";
import { cn } from "@/lib/cn";

export function GeneratorView() {
  const [mode, setMode] = useState<"password" | "login">("password");

  return (
    <div className="pms-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-6 sm:px-7">
      <div className="pms-rise mx-auto w-full max-w-xl">
        <div className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70 p-5 sm:p-6">
          <div className="mb-5">
            <h2 className="pms-display text-[26px] leading-tight">Создать секрет</h2>
            <p className="mt-1.5 text-[12px] leading-relaxed text-[var(--muted)]">
              Настройте длину и наборы символов. Каждое нажатие «Сгенерировать»
              создаёт новое значение с текущими правилами.
            </p>
          </div>

          <div className="mb-5 grid grid-cols-2 gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)] p-1">
            {(
              [
                ["password", "Пароль"],
                ["login", "Логин"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => setMode(value)}
                className={cn(
                  "h-8 rounded-[6px] text-[11.5px] transition-colors",
                  mode === value
                    ? "bg-[var(--surface)] text-[var(--text)] shadow-sm"
                    : "text-[var(--muted)] hover:text-[var(--text-soft)]",
                )}
              >
                {label}
              </button>
            ))}
          </div>

          <GeneratorPanel key={mode} mode={mode} />
        </div>
      </div>
    </div>
  );
}
