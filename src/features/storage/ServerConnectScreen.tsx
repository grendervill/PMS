import { Plus, Server } from "lucide-react";
import { Button } from "@/components/ui/Button";

type Props = {
  onExisting: () => void;
  onSetup: () => void;
  onCancel: () => void;
};

/**
 * Первый шаг серверного режима: подключиться к готовому серверу или сначала
 * его развернуть. Развёртывание можно пропустить — это и есть кнопка «У меня
 * уже есть сервер».
 */
export function ServerConnectScreen({ onExisting, onSetup, onCancel }: Props) {
  return (
    <main className="relative min-h-screen overflow-hidden bg-[var(--bg)] text-[var(--text)]">
      <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col justify-center px-6 py-12 sm:px-10">
        <button
          type="button"
          onClick={onCancel}
          className="mb-10 self-start text-[11.5px] text-[var(--muted)] transition-colors hover:text-[var(--text-soft)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
        >
          ← Назад к выбору
        </button>

        <h1 className="pms-display text-[36px] leading-[1.08] tracking-tight">Серверное хранилище</h1>
        <p className="mt-3 max-w-md text-[13.5px] leading-relaxed text-[var(--muted)]">
          Один сервер — много пользователей. У каждого свой логин и своё хранилище;
          мастер-ключ от хранилища не покидает устройство.
        </p>

        <div className="mt-9 border-t border-[var(--border)]">
          <div className="flex flex-col gap-5 border-b border-[var(--border)] py-6 sm:flex-row sm:items-start sm:gap-8">
            <div className="flex items-start gap-3 sm:w-[232px] sm:shrink-0">
              <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                <Server size={16} strokeWidth={1.7} />
              </span>
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text)]">У меня уже есть сервер</h2>
                <p className="pms-mono mt-1 text-[10.5px] text-[var(--muted)]">url · логин · ключ</p>
              </div>
            </div>
            <div className="flex-1">
              <p className="text-[13px] leading-relaxed text-[var(--text-soft)]">
                Адрес сервера, отпечаток его ключа, логин и мастер-пароль. Настройку
                сервера можно не проходить — она нужна только тем, кто его ещё не поднимал.
              </p>
              <Button size="md" variant="secondary" className="mt-4 h-10 px-4" onClick={onExisting}>
                Подключиться
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-5 border-b border-[var(--border)] py-6 sm:flex-row sm:items-start sm:gap-8">
            <div className="flex items-start gap-3 sm:w-[232px] sm:shrink-0">
              <span className="mt-0.5 grid size-9 shrink-0 place-items-center rounded-[10px] border border-[var(--border)] bg-[var(--surface-2)] text-[var(--accent-soft)]">
                <Plus size={16} strokeWidth={1.7} />
              </span>
              <div>
                <h2 className="text-[15px] font-medium text-[var(--text)]">Развернуть сервер</h2>
                <p className="pms-mono mt-1 text-[10.5px] text-[var(--muted)]">docker · tls · квота</p>
              </div>
            </div>
            <div className="flex-1">
              <p className="text-[13px] leading-relaxed text-[var(--text-soft)]">
                Готовые команды для своего VPS и выбор сертификата: Let's Encrypt через
                Caddy или самоподписанный на 10 лет. Оба варианта продлеваются сами.
              </p>
              <Button size="md" variant="secondary" className="mt-4 h-10 px-4" onClick={onSetup}>
                Показать развёртывание
              </Button>
            </div>
          </div>
        </div>

        <p className="mt-8 text-[11px] leading-relaxed text-[var(--muted)]/80">
          Сервер хранит только зашифрованные байты и счётчики места. Ни мастер-ключа,
          ни открытых данных на нём нет.
        </p>
      </div>
    </main>
  );
}
