import type {
  DocumentDraft,
  DocumentKind,
  PassportKind,
  VaultDocument,
} from "@/features/vault/types";

export const DOCUMENT_KIND_OPTIONS: readonly {
  value: DocumentKind;
  label: string;
  description: string;
}[] = [
  {
    value: "passport",
    label: "Паспорт",
    description: "Основной или загранпаспорт.",
  },
  {
    value: "snils",
    label: "СНИЛС",
    description: "Страховой номер индивидуального лицевого счёта.",
  },
  {
    value: "driver-license",
    label: "Водительское удостоверение",
    description: "Права РФ: категории, даты и подразделение.",
  },
];

export const PASSPORT_KIND_OPTIONS: readonly {
  value: PassportKind;
  label: string;
}[] = [
  { value: "main", label: "Основной" },
  { value: "foreign", label: "Загранпаспорт" },
];

export const DRIVER_CATEGORY_PRESETS = [
  "A",
  "B",
  "C",
  "D",
  "BE",
  "CE",
  "DE",
  "A1",
  "B1",
  "C1",
  "D1",
  "C1E",
  "D1E",
  "M",
  "Tm",
  "Tb",
] as const;

export function emptyDocumentDraft(kind: DocumentKind = "passport"): DocumentDraft {
  return {
    kind,
    passportKind: kind === "passport" ? "main" : null,
    fullName: "",
    series: "",
    number: "",
    issuedBy: "",
    departmentCode: "",
    issuedAt: "",
    registrationAddress: "",
    validUntil: "",
    categories: "",
    birthDate: "",
    birthPlace: "",
    attachmentIds: [],
  };
}

export function createDocument(
  draft: DocumentDraft,
  now = new Date().toISOString(),
): VaultDocument {
  return {
    ...draft,
    id: crypto.randomUUID(),
    schemaVersion: 1,
    passportKind: draft.kind === "passport" ? (draft.passportKind ?? "main") : null,
    fullName: draft.fullName.trim(),
    series: draft.series.replace(/\D/g, ""),
    number: draft.number.replace(/\D/g, ""),
    issuedBy: draft.issuedBy.trim(),
    departmentCode: draft.departmentCode.trim(),
    issuedAt: draft.issuedAt.trim(),
    registrationAddress: draft.registrationAddress.trim(),
    validUntil: draft.validUntil.trim(),
    categories: draft.categories.trim(),
    birthDate: draft.birthDate.trim(),
    birthPlace: draft.birthPlace.trim(),
    attachmentIds: [...draft.attachmentIds],
    createdAt: now,
    updatedAt: now,
  };
}

export function documentTitle(
  document: Pick<VaultDocument, "kind" | "passportKind">,
): string {
  if (document.kind === "passport") {
    return document.passportKind === "foreign" ? "Загранпаспорт" : "Паспорт РФ";
  }
  if (document.kind === "snils") return "СНИЛС";
  return "Водительское удостоверение";
}

export function documentChip(
  document: Pick<VaultDocument, "kind" | "passportKind">,
): string {
  if (document.kind === "passport") {
    return document.passportKind === "foreign" ? "Загранпаспорт" : "Основной";
  }
  if (document.kind === "snils") return "СНИЛС";
  return "Водительские права";
}

export function digitsOnly(value: string, max = 32): string {
  return value.replace(/\D/g, "").slice(0, max);
}

export function formatSnils(value: string): string {
  const digits = digitsOnly(value, 11);
  if (digits.length <= 3) return digits;
  if (digits.length <= 6) return `${digits.slice(0, 3)}-${digits.slice(3)}`;
  if (digits.length <= 9) {
    return `${digits.slice(0, 3)}-${digits.slice(3, 6)}-${digits.slice(6)}`;
  }
  return `${digits.slice(0, 3)}-${digits.slice(3, 6)}-${digits.slice(6, 9)} ${digits.slice(9)}`;
}

export function formatDepartmentCode(value: string): string {
  const digits = digitsOnly(value, 6);
  return digits.length > 3 ? `${digits.slice(0, 3)}-${digits.slice(3)}` : digits;
}

export function formatDocumentNumber(
  document: Pick<VaultDocument, "kind" | "series" | "number">,
): string {
  if (document.kind === "snils") return formatSnils(document.number);
  return [document.series, document.number].filter(Boolean).join(" ");
}

export function maskDocumentNumber(
  document: Pick<VaultDocument, "kind" | "series" | "number">,
): string {
  const digits = `${document.series}${document.number}`.replace(/\D/g, "");
  return `•••• ${digits.slice(-4)}`;
}
