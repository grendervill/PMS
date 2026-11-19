import { Button } from "@/components/ui/Button";
import { Modal } from "@/components/ui/Modal";

type Props = {
  open: boolean;
  remoteVersion: number;
  busy?: boolean;
  onKeepMine: () => void;
  onTakeTheirs: () => void;
  onLater: () => void;
};

/**
 * Конфликт версий: два устройства изменили сейф, пока не видели друг друга.
 * Ни один вариант не выбирается автоматически.
 */
export function ConflictDialog({
  open,
  remoteVersion,
  busy,
  onKeepMine,
  onTakeTheirs,
  onLater,
}: Props) {
  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) onLater();
      }}
      title="Сейф изменён на другом устройстве"
      description="Пока это устройство не синхронизировалось, хранилище успело измениться в другом месте."
      footer={
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="secondary" onClick={onLater} disabled={busy}>
            Позже
          </Button>
          <Button variant="secondary" onClick={onTakeTheirs} disabled={busy}>
            Взять серверное
          </Button>
          <Button onClick={onKeepMine} disabled={busy}>
            Оставить моё
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3 text-[13px] leading-relaxed text-[var(--text-soft)]">
        <p>
          На сервере уже <span className="text-[var(--text)]">версия {remoteVersion}</span>.
          При этом на этом устройстве есть несохранённые на сервер правки.
        </p>
        <div className="rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/60 px-3 py-2.5 text-[11.5px] leading-relaxed text-[var(--muted)]">
          «Оставить моё» отправит вашу версию поверх серверной. «Взять серверное»
          заменит локальную копию — перед этим она сохранится в <span className="pms-mono">backups/</span>.
        </div>
      </div>
    </Modal>
  );
}
