import { Loader2, Save } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Modal } from "@/components/ui/Modal";
import { useConfirm } from "@/components/ui/confirm-context";
import { useToast } from "@/components/ui/toast-context";
import { AttachmentPicker } from "@/features/attachments/AttachmentPicker";
import { errorText } from "@/features/vault/errors";
import { useVaultStore } from "@/features/vault/store";
import type {
  DocumentDraft,
  DocumentKind,
  VaultDocument,
} from "@/features/vault/types";
import {
  DOCUMENT_KIND_OPTIONS,
  DRIVER_CATEGORY_PRESETS,
  PASSPORT_KIND_OPTIONS,
  documentToDraft,
  emptyDocumentDraft,
  formatDepartmentCode,
  formatSnils,
} from "./documentModel";
import { validateDocumentDraft } from "./documentValidation";

export function DocumentEditor({
  document,
  onClose,
  onSaved,
}: {
  document: VaultDocument | null;
  onClose: () => void;
  onSaved?: () => void;
}) {
  const addDocument = useVaultStore((state) => state.addDocument);
  const updateDocument = useVaultStore((state) => state.updateDocument);
  const busy = useVaultStore((state) => state.busy);
  const toast = useToast();
  const confirm = useConfirm();
  const initial = useMemo(
    () => (document ? documentToDraft(document) : emptyDocumentDraft()),
    [document],
  );
  const [draft, setDraft] = useState<DocumentDraft>(initial);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  const dirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(initial),
    [draft, initial],
  );

  const update = <K extends keyof DocumentDraft>(
    key: K,
    value: DocumentDraft[K],
  ) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setErrors((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
  };

  const switchKind = (kind: DocumentKind) => {
    if (kind === draft.kind) return;
    setDraft({
      ...emptyDocumentDraft(kind),
      fullName: draft.fullName,
      attachmentIds: draft.attachmentIds,
    });
    setErrors({});
  };

  const activeCategories = new Set(
    draft.categories
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean),
  );

  const toggleCategory = (category: string) => {
    const next = new Set(activeCategories);
    if (next.has(category)) next.delete(category);
    else next.add(category);
    update("categories", [...next].join(", "));
  };

  const requestClose = async () => {
    if (
      dirty &&
      !(await confirm({
        title: "Не сохранять изменения?",
        description: "Введённые данные документа будут потеряны.",
        confirmLabel: "Не сохранять",
        tone: "danger",
      }))
    ) {
      return;
    }
    onClose();
  };

  const save = async () => {
    const nextErrors = validateDocumentDraft(draft);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSaving(true);
    try {
      const ok = document
        ? await updateDocument(document.id, draft)
        : await addDocument(draft);
      if (!ok) {
        toast.show({
          title: "Не удалось сохранить документ",
          description: errorText(useVaultStore.getState().error),
          tone: "error",
        });
        return;
      }
      toast.show({
        title: document ? "Документ обновлён" : "Документ добавлен",
        tone: "success",
      });
      onSaved?.();
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) void requestClose();
      }}
      title={document ? "Редактировать документ" : "Новый документ"}
      description="Выберите тип и заполните данные. Сканы можно добавить вложениями."
      className="w-[min(94vw,760px)]"
      footer={
        <>
          <Button variant="ghost" onClick={() => void requestClose()} disabled={saving}>
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
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {DOCUMENT_KIND_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              aria-pressed={draft.kind === option.value}
              onClick={() => switchKind(option.value)}
              className={
                draft.kind === option.value
                  ? "rounded-[var(--radius-md)] border border-[var(--accent)]/55 bg-[var(--accent)]/8 p-3 text-left"
                  : "rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/45 p-3 text-left hover:border-[var(--border-strong)]"
              }
            >
              <span className="block text-[12px] font-medium text-[var(--text-soft)]">
                {option.label}
              </span>
              <span className="mt-1 block text-[10px] leading-relaxed text-[var(--muted)]">
                {option.description}
              </span>
            </button>
          ))}
        </div>

        {draft.kind === "passport" ? (
          <div className="grid grid-cols-2 gap-2">
            {PASSPORT_KIND_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                aria-pressed={draft.passportKind === option.value}
                onClick={() => update("passportKind", option.value)}
                className={
                  draft.passportKind === option.value
                    ? "rounded-[var(--radius-sm)] border border-[var(--accent)]/55 bg-[var(--accent)]/8 px-3 py-2 text-[11.5px] text-[var(--accent-soft)]"
                    : "rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-2)]/45 px-3 py-2 text-[11.5px] text-[var(--muted)] hover:border-[var(--border-strong)]"
                }
              >
                {option.label}
              </button>
            ))}
          </div>
        ) : null}

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="ФИО" htmlFor="doc-full-name" error={errors.fullName}>
            <Input
              id="doc-full-name"
              value={draft.fullName}
              onChange={(event) => update("fullName", event.target.value)}
              placeholder="ИВАНОВ ИВАН ИВАНОВИЧ"
            />
          </Field>

          {draft.kind === "snils" ? (
            <Field label="Номер СНИЛС" htmlFor="doc-snils" error={errors.number}>
              <Input
                id="doc-snils"
                value={formatSnils(draft.number)}
                onChange={(event) =>
                  update("number", event.target.value.replace(/\D/g, "").slice(0, 11))
                }
                placeholder="000-000-000 00"
                inputMode="numeric"
                className="pms-mono"
              />
            </Field>
          ) : (
            <>
              <Field
                label="Серия"
                htmlFor="doc-series"
                error={errors.series}
                hint={
                  draft.kind === "passport" && draft.passportKind === "foreign"
                    ? "2 цифры"
                    : "4 цифры"
                }
              >
                <Input
                  id="doc-series"
                  value={draft.series}
                  onChange={(event) =>
                    update("series", event.target.value.replace(/\D/g, "").slice(0, 4))
                  }
                  placeholder="0000"
                  inputMode="numeric"
                  className="pms-mono"
                />
              </Field>
              <Field label="Номер" htmlFor="doc-number" error={errors.number}>
                <Input
                  id="doc-number"
                  value={draft.number}
                  onChange={(event) =>
                    update("number", event.target.value.replace(/\D/g, "").slice(0, 7))
                  }
                  placeholder="000000"
                  inputMode="numeric"
                  className="pms-mono"
                />
              </Field>
            </>
          )}
        </div>

        {draft.kind !== "snils" ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Кем выдан" htmlFor="doc-issued-by" error={errors.issuedBy}>
              <Input
                id="doc-issued-by"
                value={draft.issuedBy}
                onChange={(event) => update("issuedBy", event.target.value)}
                placeholder="Наименование органа"
              />
            </Field>
            <Field
              label="Код подразделения"
              htmlFor="doc-department"
              error={errors.departmentCode}
              hint={
                draft.kind === "passport" && draft.passportKind === "foreign"
                  ? "Необязательно"
                  : "Формат XXX-XXX"
              }
            >
              <Input
                id="doc-department"
                value={formatDepartmentCode(draft.departmentCode)}
                onChange={(event) =>
                  update("departmentCode", formatDepartmentCode(event.target.value))
                }
                placeholder="000-000"
                inputMode="numeric"
                className="pms-mono"
              />
            </Field>
            <Field label="Дата выдачи" htmlFor="doc-issued-at" error={errors.issuedAt}>
              <Input
                id="doc-issued-at"
                type="date"
                value={draft.issuedAt}
                onChange={(event) => update("issuedAt", event.target.value)}
              />
            </Field>
            {draft.kind === "driver-license" ? (
              <Field
                label="Действует до"
                htmlFor="doc-valid-until"
                error={errors.validUntil}
              >
                <Input
                  id="doc-valid-until"
                  type="date"
                  value={draft.validUntil}
                  onChange={(event) => update("validUntil", event.target.value)}
                />
              </Field>
            ) : null}
          </div>
        ) : null}

        {draft.kind === "driver-license" ? (
          <div className="grid gap-4">
            <div className="grid gap-2">
              <label
                htmlFor="doc-categories"
                className="text-[13px] font-medium text-[var(--text-soft)]"
              >
                Категории
              </label>
              <div className="flex flex-wrap gap-1.5">
                {DRIVER_CATEGORY_PRESETS.map((category) => (
                  <button
                    key={category}
                    type="button"
                    aria-pressed={activeCategories.has(category)}
                    onClick={() => toggleCategory(category)}
                    className={
                      activeCategories.has(category)
                        ? "rounded-full border border-[var(--accent)]/50 bg-[var(--accent)]/10 px-2.5 py-1 text-[11px] text-[var(--accent-soft)]"
                        : "rounded-full border border-[var(--border)] bg-[var(--surface-2)]/60 px-2.5 py-1 text-[11px] text-[var(--muted)] hover:border-[var(--border-strong)]"
                    }
                  >
                    {category}
                  </button>
                ))}
              </div>
              <Input
                id="doc-categories"
                value={draft.categories}
                onChange={(event) => update("categories", event.target.value)}
                placeholder="A, B, M"
              />
              {errors.categories ? (
                <p role="alert" className="text-xs text-[var(--danger)]">
                  {errors.categories}
                </p>
              ) : null}
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field
                label="Дата рождения"
                htmlFor="doc-birth-date"
                error={errors.birthDate}
              >
                <Input
                  id="doc-birth-date"
                  type="date"
                  value={draft.birthDate}
                  onChange={(event) => update("birthDate", event.target.value)}
                />
              </Field>
              <Field label="Место рождения" htmlFor="doc-birth-place">
                <Input
                  id="doc-birth-place"
                  value={draft.birthPlace}
                  onChange={(event) => update("birthPlace", event.target.value)}
                  placeholder="Город, регион"
                />
              </Field>
            </div>
          </div>
        ) : null}

        {draft.kind !== "snils" ? (
          <Field
            label={draft.kind === "passport" ? "Адрес прописки" : "Адрес регистрации"}
            htmlFor="doc-registration-address"
            error={
              draft.kind === "passport" && draft.passportKind === "main"
                ? errors.registrationAddress
                : undefined
            }
          >
            <Input
              id="doc-registration-address"
              value={draft.registrationAddress}
              onChange={(event) => update("registrationAddress", event.target.value)}
              placeholder="Город, улица, дом, квартира"
            />
          </Field>
        ) : null}

        <AttachmentPicker
          ids={draft.attachmentIds}
          onChange={(ids) => update("attachmentIds", ids)}
        />
      </div>
    </Modal>
  );
}
