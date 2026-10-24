import { Pencil, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { CopyButton } from "@/components/ui/CopyButton";
import { DetailRow } from "@/components/ui/DetailRow";
import { Modal } from "@/components/ui/Modal";
import { AttachmentsSection } from "@/features/attachments/AttachmentsSection";
import type { VaultDocument } from "@/features/vault/types";
import {
  documentChip,
  documentTitle,
  formatDocumentDate,
  formatDocumentNumber,
} from "./documentModel";

export function DocumentDetail({
  document,
  onClose,
  onEdit,
  onDelete,
}: {
  document: VaultDocument;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={documentTitle(document)}
      description="Данные документа и оригиналы во вложениях."
      className="w-[min(94vw,720px)]"
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
        <span className="w-fit rounded-full border border-[var(--accent)]/30 bg-[var(--accent)]/8 px-2.5 py-1 text-[10.5px] text-[var(--accent-soft)]">
          {documentChip(document)}
        </span>

        <div className="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-2)]/45 px-4 py-2">
          {document.fullName ? (
            <DetailRow
              label="ФИО"
              value={document.fullName}
              action={<CopyButton value={document.fullName} label="Копировать ФИО" />}
            />
          ) : null}
          <DetailRow
            label="Номер"
            value={
              <span className="pms-mono">
                {formatDocumentNumber(document)}
              </span>
            }
            action={
              <CopyButton
                value={formatDocumentNumber(document)}
                label="Копировать номер"
              />
            }
          />
          {document.issuedBy ? (
            <DetailRow
              label="Кем выдан"
              value={document.issuedBy}
              action={
                <CopyButton
                  value={document.issuedBy}
                  label="Копировать, кем выдан"
                />
              }
            />
          ) : null}
          {document.departmentCode ? (
            <DetailRow
              label="Код подразделения"
              value={document.departmentCode}
              action={
                <CopyButton
                  value={document.departmentCode}
                  label="Копировать код подразделения"
                />
              }
            />
          ) : null}
          {document.issuedAt ? (
            <DetailRow
              label="Дата выдачи"
              value={formatDocumentDate(document.issuedAt)}
              action={
                <CopyButton
                  value={formatDocumentDate(document.issuedAt)}
                  label="Копировать дату выдачи"
                />
              }
            />
          ) : null}
          {document.validUntil ? (
            <DetailRow
              label="Действует до"
              value={formatDocumentDate(document.validUntil)}
              action={
                <CopyButton
                  value={formatDocumentDate(document.validUntil)}
                  label="Копировать срок действия"
                />
              }
            />
          ) : null}
          {document.categories ? (
            <DetailRow
              label="Категории"
              value={document.categories}
              action={
                <CopyButton
                  value={document.categories}
                  label="Копировать категории"
                />
              }
            />
          ) : null}
          {document.birthDate ? (
            <DetailRow
              label="Дата рождения"
              value={formatDocumentDate(document.birthDate)}
              action={
                <CopyButton
                  value={formatDocumentDate(document.birthDate)}
                  label="Копировать дату рождения"
                />
              }
            />
          ) : null}
          {document.birthPlace ? (
            <DetailRow
              label="Место рождения"
              value={document.birthPlace}
              action={
                <CopyButton
                  value={document.birthPlace}
                  label="Копировать место рождения"
                />
              }
            />
          ) : null}
          {document.registrationAddress ? (
            <DetailRow
              label={
                document.kind === "passport"
                  ? "Адрес прописки"
                  : "Адрес регистрации"
              }
              value={document.registrationAddress}
              action={
                <CopyButton
                  value={document.registrationAddress}
                  label="Копировать адрес"
                />
              }
            />
          ) : null}
        </div>

        <AttachmentsSection attachmentIds={document.attachmentIds} />
      </div>
    </Modal>
  );
}
