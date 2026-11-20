import { CloudOff, Database, RefreshCw, ShieldOff, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import type { ServerProfile } from "@/features/storage/profiles";
import type { ServerUsage } from "@/features/vault/types";
import type { SyncStatus } from "@/features/sync/controller";

type Props = {
  profile: ServerProfile;
  status: SyncStatus;
  usage: ServerUsage | null;
  stale: boolean;
  onSyncNow: () => void;
  onDisconnect: () => void;
  onDeleteAccount: () => void;
};

function bytes(value: number): string {
  if (value >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} ГиБ`;
  if (value >= 1024 ** 2) return `${Math.round(value / 1024 ** 2)} МиБ`;
  return `${Math.round(value / 1024)} КиБ`;
}

function statusText(status: SyncStatus): { text: string; tone: string } {
  switch (status.kind) {
    case "syncing":
      return { text: "Синхронизация…", tone: "text-[var(--accent-soft)]" };
    case "dirty":
      return { text: "Есть неотправленное", tone: "text-[var(--warning)]" };
    case "offline":
      return { text: `Офлайн: ${status.message}`, tone: "text-[var(--warning)]" };
    case "conflict":
      return { text: "Конфликт версий", tone: "text-[var(--danger)]" };
    case "error":
      return { text: `Ошибка: ${status.message}`, tone: "text-[var(--danger)]" };
    default:
      return { text: "Синхронизировано", tone: "text-[var(--accent-soft)]" };
  }
}

export function ServerSection({
  profile,
  status,
  usage,
  stale,
  onSyncNow,
  onDisconnect,
  onDeleteAccount,
}: Props) {
  const used = usage?.used ?? 0;
  const quota = usage?.quota ?? 0;
  const share = quota > 0 ? Math.min(100, Math.round((used / quota) * 100)) : 0;
  const tone = statusText(status);
  const tight = share >= 90;

  return (
    <section className="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface)]/70">
      <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-3">
        <Database size={15} className="text-[var(--accent-soft)]" />
        <h2 className="pms-display text-lg">Серверное хранилище</h2>
      </div>

      <div className="grid gap-4 px-4 py-4">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <div>
            <p className="text-[13px] font-medium text-[var(--text-soft)]">{profile.name}</p>
            <p className="pms-mono mt-0.5 text-[10.5px] text-[var(--muted)]">
              {profile.login} · {profile.baseUrl}
            </p>
          </div>
          <p className={`text-[11.5px] ${tone.tone}`}>{tone.text}</p>
        </div>

        <div>
          <div className="mb-1.5 flex items-baseline justify-between gap-2">
            <p className="text-[11.5px] text-[var(--text-soft)]">
              {bytes(used)} из {bytes(quota)}
              {stale ? <span className="ml-1.5 text-[var(--muted)]">· устарело</span> : null}
            </p>
            <p className={`text-[11px] ${tight ? "text-[var(--danger)]" : "text-[var(--muted)]"}`}>
              {share}%
            </p>
          </div>
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={share}
            aria-label="Занятое место на сервере"
            className="h-2 w-full overflow-hidden rounded-[var(--radius-pill)] border border-[var(--border)] bg-[var(--surface-2)]"
          >
            <div
              className={`h-full rounded-[var(--radius-pill)] transition-[width] duration-[var(--motion-base)] ${
                tight
                  ? "bg-[var(--danger)]"
                  : "bg-[linear-gradient(90deg,var(--accent)_0%,var(--accent-strong)_100%)]"
              }`}
              style={{ width: `${share}%` }}
            />
          </div>
          <p className="mt-1.5 text-[10.5px] text-[var(--muted)]">
            Сейф и вложения вместе. Квота задаётся на сервере.
          </p>
        </div>

        <div className="flex flex-wrap gap-2 border-t border-[var(--border)] pt-3.5">
          <Button variant="secondary" onClick={onSyncNow}>
            <RefreshCw size={15} />
            Синхронизировать сейчас
          </Button>
          <Button variant="secondary" onClick={onDisconnect}>
            <CloudOff size={15} />
            Отключиться
          </Button>
          <Button variant="secondary" onClick={onDeleteAccount}>
            <Trash2 size={15} />
            Удалить аккаунт на сервере
          </Button>
        </div>

        <p className="flex items-start gap-1.5 text-[10.5px] leading-relaxed text-[var(--muted)]/85">
          <ShieldOff size={13} className="mt-0.5 shrink-0" />
          При отключении локальная копия останется — её можно сохранить в обычную папку.
        </p>
      </div>
    </section>
  );
}
