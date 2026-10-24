import type { BankCard, BankCardDraft, CardImagePreset } from "@/features/vault/types";

export const CARD_IMAGE_PRESETS: readonly {
  id: CardImagePreset;
  label: string;
}[] = [
  { id: "emerald", label: "Изумруд" },
  { id: "graphite", label: "Графит" },
  { id: "midnight", label: "Полночь" },
  { id: "oxblood", label: "Вино" },
  { id: "sand", label: "Песок" },
  { id: "copper", label: "Медь" },
];

export const CARD_PURPOSE_PRESETS = [
  "Основная",
  "Для подписок",
  "Накопительная",
  "Кредитная",
  "Дебетовая",
] as const;

export const DEFAULT_CARD_PRESET: CardImagePreset = "emerald";

export function emptyCardDraft(): BankCardDraft {
  return {
    number: "",
    expiry: "",
    cvv: "",
    holder: "",
    purpose: "",
    country: "",
    address: "",
    postalCode: "",
    imagePreset: DEFAULT_CARD_PRESET,
    imageAttachmentId: null,
  };
}

export function createBankCard(
  draft: BankCardDraft,
  now = new Date().toISOString(),
): BankCard {
  return {
    ...draft,
    id: crypto.randomUUID(),
    schemaVersion: 1,
    number: draft.number.replace(/\D/g, ""),
    expiry: draft.expiry.trim(),
    cvv: draft.cvv.trim(),
    holder: draft.holder.trim(),
    purpose: draft.purpose.trim(),
    country: draft.country.trim(),
    address: draft.address.trim(),
    postalCode: draft.postalCode.trim(),
    createdAt: now,
    updatedAt: now,
  };
}

export function cardLabel(
  card: Pick<BankCard, "number" | "purpose">,
): string {
  const purpose = card.purpose.trim() || "Банковская карта";
  const last4 = card.number.replace(/\D/g, "").slice(-4);
  return last4 ? `${purpose} · ${last4}` : purpose;
}

export function cardToDraft(card: BankCard): BankCardDraft {
  return {
    number: card.number,
    expiry: card.expiry,
    cvv: card.cvv,
    holder: card.holder,
    purpose: card.purpose,
    country: card.country,
    address: card.address,
    postalCode: card.postalCode,
    imagePreset: card.imagePreset,
    imageAttachmentId: card.imageAttachmentId,
  };
}
