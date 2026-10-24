import { Pencil, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { Modal } from "@/components/ui/Modal";
import { useAttachmentUrl } from "@/features/attachments/useAttachmentUrl";
import type { BankCard } from "@/features/vault/types";
import { BankCardArt } from "./BankCardArt";
import { cardLabel } from "./cardModel";
import { formatCardNumber } from "./cardValidation";

function Row({
  label,
  value,
  action,
}: {
  label: string;
  value: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3 border-b border-[var(--border)]/70 py-2 last:border-b-0">
      <span className="shrink-0 text-[11px] text-[var(--muted)]">{label}</span>
      <span className="flex min-w-0 items-center gap-1 text-right text-[12px] text-[var(--text-soft)]">
        <span className="min-w-0 break-words">{value}</span>
        {action}
      </span>
    </div>
  );
}

export function CardDetail({
  card,
  onClose,
  onEdit,
  onDelete,
}: {
  card: BankCard;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [flipped, setFlipped] = useState(false);
  const imageUrl = useAttachmentUrl(card.imageAttachmentId);

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={cardLabel(card)}
      description="Нажмите на карту, чтобы увидеть срок и CVV."
      className="w-[min(94vw,760px)]"
      footer={
        <>
          <Button
            variant="ghost"
            className="mr-auto text-[var(--danger)]"
            onClick={onDelete}
          >
            <Trash2 size={14} />
            Удалить
          </Button>
          <Button variant="secondary" onClick={onEdit}>
            <Pencil size={14} />
            Редактировать
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <button
          type="button"
          aria-label={
            flipped ? "Показать лицевую сторону" : "Показать обратную сторону"
          }
          aria-pressed={flipped}
          onClick={() => setFlipped((current) => !current)}
          className="mx-auto block w-full max-w-[520px] rounded-[18px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
        >
          <BankCardArt
            card={card}
            imageUrl={imageUrl}
            revealNumber
            flipped={flipped}
          />
        </button>
        <p className="text-center text-[10.5px] text-[var(--muted)]">
          {flipped
            ? "Нажмите, чтобы вернуться к лицевой стороне"
            : "Нажмите, чтобы перевернуть карту"}
        </p>

        <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/45 px-4 py-2">
          <Row
            label="Номер"
            value={
              <span className="pms-mono">{formatCardNumber(card.number)}</span>
            }
            action={<CopyButton value={card.number} label="Копировать номер" />}
          />
          <Row label="Владелец" value={card.holder} />
          <Row
            label="Срок"
            value={flipped ? card.expiry : "На обороте"}
          />
          <Row
            label="CVV"
            value={flipped ? card.cvv : "На обороте"}
            action={
              flipped ? (
                <CopyButton value={card.cvv} label="Копировать CVV" />
              ) : undefined
            }
          />
          {card.purpose ? <Row label="Назначение" value={card.purpose} /> : null}
          {card.country ? <Row label="Страна" value={card.country} /> : null}
          {card.address ? <Row label="Адрес" value={card.address} /> : null}
          {card.postalCode ? (
            <Row label="Индекс" value={card.postalCode} />
          ) : null}
        </div>
      </div>
    </Modal>
  );
}
