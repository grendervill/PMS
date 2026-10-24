import type { BankCard } from "@/features/vault/types";
import { formatCardNumber, maskCardNumber } from "./cardValidation";
import { cn } from "@/lib/cn";
import "./BankCardArt.css";

export function BankCardArt({
  card,
  imageUrl,
  revealNumber,
  flipped,
  className,
}: {
  card: BankCard;
  imageUrl: string | null;
  revealNumber: boolean;
  flipped: boolean;
  className?: string;
}) {
  const number = revealNumber
    ? formatCardNumber(card.number)
    : maskCardNumber(card.number);
  const imageStyle = imageUrl
    ? { backgroundImage: `url("${imageUrl}")` }
    : undefined;

  return (
    <div
      aria-hidden="true"
      className={cn(
        "pms-card-art",
        flipped && "pms-card-art--flipped",
        className,
      )}
      data-preset={card.imagePreset}
      data-custom={imageUrl ? "true" : undefined}
    >
      <div className="pms-card-art__inner">
        <div
          className="pms-card-art__face pms-card-art__front"
          style={imageStyle}
        >
          <span className="pms-card-art__shade" />
          {card.purpose ? (
            <span className="pms-card-art__chip">{card.purpose}</span>
          ) : null}
          <span className="pms-card-art__identity">
            <span className="pms-card-art__number">{number}</span>
            <span className="pms-card-art__holder">{card.holder}</span>
          </span>
        </div>

        <div
          className="pms-card-art__face pms-card-art__back"
          style={imageStyle}
        >
          <span className="pms-card-art__back-shade" />
          <span className="pms-card-art__stripe" />
          <span className="pms-card-art__back-data">
            <span className="pms-card-art__back-field">
              <span className="pms-card-art__label">Срок</span>
              <span className="pms-card-art__value">{card.expiry}</span>
            </span>
            <span className="pms-card-art__back-field pms-card-art__back-field--right">
              <span className="pms-card-art__label">CVV</span>
              <span className="pms-card-art__value">{card.cvv}</span>
            </span>
          </span>
        </div>
      </div>
    </div>
  );
}
