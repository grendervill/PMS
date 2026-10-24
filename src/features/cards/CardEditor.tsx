import { Loader2, Save } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Modal } from "@/components/ui/Modal";
import { SecretInput } from "@/components/ui/SecretInput";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { useAttachmentUrl } from "@/features/attachments/useAttachmentUrl";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type { BankCard, BankCardDraft } from "@/features/vault/types";
import { BankCardArt } from "./BankCardArt";
import { CardImagePicker } from "./CardImagePicker";
import {
  CARD_PURPOSE_PRESETS,
  cardToDraft,
  emptyCardDraft,
} from "./cardModel";
import {
  digitsOnly,
  formatCardNumber,
  formatExpiryInput,
  normalizeHolder,
  validateCardDraft,
} from "./cardValidation";

function draftToPreview(draft: BankCardDraft, card: BankCard | null): BankCard {
  return {
    ...draft,
    id: card?.id ?? "preview",
    schemaVersion: card?.schemaVersion ?? 1,
    createdAt: card?.createdAt ?? "",
    updatedAt: card?.updatedAt ?? "",
  };
}

export function CardEditor({
  card,
  onClose,
  onSaved,
}: {
  card: BankCard | null;
  onClose: () => void;
  onSaved?: () => void;
}) {
  const addBankCard = useVaultStore((state) => state.addBankCard);
  const updateBankCard = useVaultStore((state) => state.updateBankCard);
  const busy = useVaultStore((state) => state.busy);
  const toast = useToast();
  const confirm = useConfirm();
  const initial = useMemo(
    () => (card ? cardToDraft(card) : emptyCardDraft()),
    [card],
  );
  const [draft, setDraft] = useState<BankCardDraft>(initial);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [imageError, setImageError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const imageUrl = useAttachmentUrl(draft.imageAttachmentId);

  const dirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(initial),
    [draft, initial],
  );

  const update = <K extends keyof BankCardDraft>(
    key: K,
    value: BankCardDraft[K],
  ) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setErrors((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
    if (key === "imageAttachmentId") setImageError(null);
  };

  const requestClose = async () => {
    if (
      dirty &&
      !(await confirm({
        title: "Не сохранять изменения?",
        description: "Введённые данные карты будут потеряны.",
        confirmLabel: "Не сохранять",
        tone: "danger",
      }))
    ) {
      return;
    }
    onClose();
  };

  const save = async () => {
    const nextErrors = validateCardDraft(draft);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSaving(true);
    try {
      const ok = card
        ? await updateBankCard(card.id, draft)
        : await addBankCard(draft);
      if (!ok) {
        toast.show({
          title: "Не удалось сохранить карту",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
        return;
      }
      toast.show({
        title: card ? "Карта обновлена" : "Карта добавлена",
        tone: "success",
      });
      onSaved?.();
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const preview = useMemo(() => draftToPreview(draft, card), [draft, card]);

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) void requestClose();
      }}
      title={card ? "Редактировать карту" : "Новая карта"}
      description="Номер, срок, CVV и имя владельца обязательны."
      className="w-[min(94vw,760px)]"
      footer={
        <>
          <Button
            variant="ghost"
            onClick={() => void requestClose()}
            disabled={saving}
          >
            Отменить
          </Button>
          <Button onClick={() => void save()} disabled={saving || busy}>
            {saving ? <Loader2 size={15} className="animate-spin" /> : <Save size={15} />}
            Сохранить
          </Button>
        </>
      }
    >
      <div className="grid gap-5">
        <div className="mx-auto w-full max-w-[460px]">
          <BankCardArt
            card={preview}
            imageUrl={imageUrl}
            revealNumber
            flipped={false}
          />
        </div>

        <CardImagePicker
          preset={draft.imagePreset}
          attachmentId={draft.imageAttachmentId}
          imageUrl={imageUrl}
          onPreset={(preset) => update("imagePreset", preset)}
          onAttachment={(id) => update("imageAttachmentId", id)}
          onError={setImageError}
        />
        {imageError ? (
          <p role="alert" className="text-xs text-[var(--danger)]">
            {imageError}
          </p>
        ) : null}

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Номер карты" htmlFor="card-number" error={errors.number}>
            <Input
              id="card-number"
              value={formatCardNumber(draft.number)}
              onChange={(event) =>
                update("number", digitsOnly(event.target.value))
              }
              placeholder="0000 0000 0000 0000"
              inputMode="numeric"
              autoComplete="cc-number"
              className="pms-mono"
            />
          </Field>
          <Field label="Срок действия" htmlFor="card-expiry" error={errors.expiry}>
            <Input
              id="card-expiry"
              value={formatExpiryInput(draft.expiry)}
              onChange={(event) =>
                update("expiry", formatExpiryInput(event.target.value))
              }
              placeholder="ММ/ГГ"
              inputMode="numeric"
              autoComplete="cc-exp"
              className="pms-mono"
            />
          </Field>
          <Field label="CVV" htmlFor="card-cvv" error={errors.cvv}>
            <SecretInput
              id="card-cvv"
              value={draft.cvv}
              onChange={(event) =>
                update("cvv", digitsOnly(event.target.value, 4))
              }
              placeholder="123"
              inputMode="numeric"
              maxLength={4}
              autoComplete="cc-csc"
            />
          </Field>
          <Field
            label="Имя владельца"
            htmlFor="card-holder"
            error={errors.holder}
            hint="Латиницей: ИМЯ ФАМИЛИЯ."
          >
            <Input
              id="card-holder"
              value={draft.holder}
              onChange={(event) =>
                update("holder", normalizeHolder(event.target.value))
              }
              placeholder="IVAN IVANOV"
              autoComplete="cc-name"
            />
          </Field>
        </div>

        <div className="grid gap-2">
          <span className="text-[13px] font-medium text-[var(--text-soft)]">
            Назначение
          </span>
          <div className="flex flex-wrap gap-1.5">
            {CARD_PURPOSE_PRESETS.map((purpose) => (
              <button
                key={purpose}
                type="button"
                aria-pressed={draft.purpose === purpose}
                onClick={() => update("purpose", purpose)}
                className={
                  draft.purpose === purpose
                    ? "rounded-full border border-[var(--accent)]/50 bg-[var(--accent)]/10 px-2.5 py-1 text-[11px] text-[var(--accent-soft)]"
                    : "rounded-full border border-[var(--border)] bg-[var(--surface-2)]/60 px-2.5 py-1 text-[11px] text-[var(--muted)] hover:border-[var(--border-strong)]"
                }
              >
                {purpose}
              </button>
            ))}
          </div>
          <Input
            aria-label="Своё назначение"
            value={draft.purpose}
            onChange={(event) => update("purpose", event.target.value)}
            placeholder="Или своё значение"
          />
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Страна" htmlFor="card-country">
            <Input
              id="card-country"
              value={draft.country}
              onChange={(event) => update("country", event.target.value)}
              placeholder="Россия"
            />
          </Field>
          <Field label="Почтовый индекс" htmlFor="card-postal">
            <Input
              id="card-postal"
              value={draft.postalCode}
              onChange={(event) => update("postalCode", event.target.value)}
              placeholder="101000"
            />
          </Field>
          <Field label="Адрес" htmlFor="card-address" className="sm:col-span-2">
            <Input
              id="card-address"
              value={draft.address}
              onChange={(event) => update("address", event.target.value)}
              placeholder="Город, улица, дом"
            />
          </Field>
        </div>
      </div>
    </Modal>
  );
}
