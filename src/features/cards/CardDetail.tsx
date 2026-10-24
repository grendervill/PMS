import { Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { DetailRow } from "@/components/ui/DetailRow";
import { Modal } from "@/components/ui/Modal";
import { useAttachmentUrl } from "@/features/attachments/useAttachmentUrl";
import type { BankCard } from "@/features/vault/types";
import { BankCardArt } from "./BankCardArt";
import { cardLabel } from "./cardModel";
import { formatCardNumber } from "./cardValidation";

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
          <DetailRow
            label="Номер"
            value={
              <span className="pms-mono">{formatCardNumber(card.number)}</span>
            }
            action={<CopyButton value={card.number} label="Копировать номер" />}
          />
          <DetailRow
            label="Владелец"
            value={card.holder}
            action={<CopyButton value={card.holder} label="Копировать владельца" />}
          />
          <DetailRow
            label="Срок"
            value={flipped ? card.expiry : "На обороте"}
            action={
              flipped ? (
                <CopyButton value={card.expiry} label="Копировать срок" />
              ) : undefined
            }
          />
          <DetailRow
            label="CVV"
            value={flipped ? card.cvv : "На обороте"}
            action={
              flipped ? (
                <CopyButton value={card.cvv} label="Копировать CVV" />
              ) : undefined
            }
          />
          {card.purpose ? (
            <DetailRow
              label="Назначение"
              value={card.purpose}
              action={<CopyButton value={card.purpose} label="Копировать назначение" />}
            />
          ) : null}
          {card.country ? (
            <DetailRow
              label="Страна"
              value={card.country}
              action={<CopyButton value={card.country} label="Копировать страну" />}
            />
          ) : null}
          {card.address ? (
            <DetailRow
              label="Адрес"
              value={card.address}
              action={<CopyButton value={card.address} label="Копировать адрес" />}
            />
          ) : null}
          {card.postalCode ? (
            <DetailRow
              label="Индекс"
              value={card.postalCode}
              action={<CopyButton value={card.postalCode} label="Копировать индекс" />}
            />
          ) : null}
        </div>
      </div>
    </Modal>
  );
}
