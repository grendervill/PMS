import { HardDrive, Server } from "lucide-react";
import { Button } from "@/components/ui/Button";

type Props = {
  onLocal: () => void;
  onServer: () => void;
};

/**
 * Первый экран: где живёт сейф. Один вопрос, два честных ответа, никаких
 * настроек — остальное появится после выбора.
 */
export function StorageModeScreen({ onLocal, onServer }: Props) {
  return (
    <main className="relative min-h-screen overflow-hidden bg-[var(--bg)] text-[var(--text)]">
      <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col justify-center px-6 py-12 sm:px-10">
        <header className="flex items-center gap-3">
          <div className="grid size-11 place-items-center rounded-[12px] border border-[var(--border-strong)] bg-[var(--surface-2)] text-[var(--accent)]">
            <svg viewBox="0 0 24 24" className="size-5" aria-hidden="true">
              <path
                d="M6 10V8a6 6 0 1 1 12 0v2"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.7"
                strokeLinecap="round"
              />
              <rect
                x="4"
                y="10"
                width="16"
                height="10"
                rx="2.5"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.7"
              />
            </svg>
          </div>
          <div>
            <p className="pms-display text-[22px] leading-none tracking-tight">PMS</p>
            <p className="mt-1 text-[11px] text-[var(--muted)]">Личное хранилище</p>
          </div>
        </header>

        <h1 className="pms-display mt-12 text-[40px] leading-[1.05] tracking-tight sm:text-[46px]">
          Где живёт ваш сейф?
        </h1>
        <p className="mt-4 max-w-md text-[14px] leading-relaxed text-[var(--muted)]">
          Это единственный выбор, который нужно сделать сейчас. Переносить хранилище
          между способами можно и позже.
        </p>

        <div className="mt-10 border-t border-[var(--border)]">
          <div className="flex flex-col gap-5 border-b border-[var(--border)] py-6 sm:flex-row sm:items-start sm:gap-8">
            <div className="flex items-start gap-3 sm:w-[232px] sm:shrink-0">
              <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                <HardDrive size={16} strokeWidth={1.7} />
              </span>
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text)]">Папка на диске</h2>
                <p className="pms-mono mt-1 text-[10.5px] text-[var(--muted)]">
                  pms.vault · attachments/
                </p>
              </div>
            </div>
            <div className="flex-1">
              <p className="text-[13px] leading-relaxed text-[var(--text-soft)]">
                Сейф целиком в выбранной папке. Работает без сети, переносится копированием,
                ничего никуда не отправляет.
              </p>
              <Button
                size="md"
                variant="secondary"
                className="mt-4 h-10 px-4"
                onClick={onLocal}
              >
                Работать с папкой
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-5 border-b border-[var(--border)] py-6 sm:flex-row sm:items-start sm:gap-8">
            <div className="flex items-start gap-3 sm:w-[232px] sm:shrink-0">
              <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                <Server size={16} strokeWidth={1.7} />
              </span>
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text)]">Свой сервер</h2>
                <p className="pms-mono mt-1 text-[10.5px] text-[var(--muted)]">
                  https · свой сервер
                </p>
              </div>
            </div>
            <div className="flex-1">
              <p className="text-[13px] leading-relaxed text-[var(--text-soft)]">
                Одно хранилище на всех ваших устройствах. Сервер хранит только
                зашифрованные байты — ни пароль-фразы, ни открытых данных у него нет.
              </p>
              <Button
                size="md"
                variant="secondary"
                className="mt-4 h-10 px-4"
                onClick={onServer}
              >
                Подключить сервер
              </Button>
            </div>
          </div>
        </div>

        <p className="mt-8 text-[11px] leading-relaxed text-[var(--muted)]/80">
          Выбор запомним, чтобы не спрашивать при каждом запуске. Поменять его можно
          в настройках.
        </p>
      </div>
    </main>
  );
}
