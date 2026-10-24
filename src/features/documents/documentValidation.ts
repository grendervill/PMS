import type { DocumentDraft } from "@/features/vault/types";

function isDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(Date.parse(value));
}

function digits(value: string): string {
  return value.replace(/\D/g, "");
}

export function validateDocumentDraft(
  draft: DocumentDraft,
  today = new Date().toISOString().slice(0, 10),
): Record<string, string> {
  const errors: Record<string, string> = {};
  const passport = draft.kind === "passport";
  const mainPassport = passport && (draft.passportKind ?? "main") === "main";
  const foreignPassport = passport && draft.passportKind === "foreign";

  if (draft.kind === "snils") {
    const number = digits(draft.number);
    if (!number) {
      errors.number = "Введите номер СНИЛС";
    } else if (number.length !== 11) {
      errors.number = "СНИЛС должен содержать 11 цифр";
    }
    return errors;
  }

  if (passport) {
    const expectedSeries = foreignPassport ? 2 : 4;
    const expectedNumber = foreignPassport ? 7 : 6;
    const series = digits(draft.series);
    const number = digits(draft.number);

    if (!series) {
      errors.series = "Введите серию паспорта";
    } else if (series.length !== expectedSeries) {
      errors.series = foreignPassport
        ? "Серия загранпаспорта должна содержать 2 цифры"
        : "Серия паспорта должна содержать 4 цифры";
    }

    if (!number) {
      errors.number = "Введите номер паспорта";
    } else if (number.length !== expectedNumber) {
      errors.number = foreignPassport
        ? "Номер загранпаспорта должен содержать 7 цифр"
        : "Номер паспорта должен содержать 6 цифр";
    }

    if (!draft.issuedBy.trim()) errors.issuedBy = "Введите, кем выдан документ";
    if (mainPassport) {
      if (!draft.departmentCode.trim()) {
        errors.departmentCode = "Введите код подразделения";
      } else if (!/^\d{3}-\d{3}$/.test(draft.departmentCode.trim())) {
        errors.departmentCode = "Код подразделения должен быть в формате XXX-XXX";
      }
      if (!draft.registrationAddress.trim()) {
        errors.registrationAddress = "Введите адрес прописки";
      }
    }

    validateIssuedAt(draft.issuedAt, today, errors);
    return errors;
  }

  const series = digits(draft.series);
  const number = digits(draft.number);
  if (!series) {
    errors.series = "Введите серию";
  } else if (series.length !== 4) {
    errors.series = "Серия должна содержать 4 цифры";
  }
  if (!number) {
    errors.number = "Введите номер";
  } else if (number.length !== 6) {
    errors.number = "Номер должен содержать 6 цифр";
  }
  if (!draft.fullName.trim() || draft.fullName.trim().split(/\s+/).length < 2) {
    errors.fullName = "Введите имя и фамилию";
  }
  if (!draft.issuedBy.trim()) errors.issuedBy = "Введите, кем выдан документ";
  if (!draft.categories.trim()) errors.categories = "Укажите категории";
  validateIssuedAt(draft.issuedAt, today, errors);

  if (!draft.validUntil.trim()) {
    errors.validUntil = "Введите срок действия";
  } else if (!isDate(draft.validUntil)) {
    errors.validUntil = "Некорректная дата окончания";
  } else if (isDate(draft.issuedAt) && draft.validUntil < draft.issuedAt) {
    errors.validUntil = "Срок действия не может быть раньше даты выдачи";
  }

  if (draft.birthDate) {
    if (!isDate(draft.birthDate)) {
      errors.birthDate = "Некорректная дата рождения";
    } else if (draft.birthDate > today) {
      errors.birthDate = "Дата рождения не может быть в будущем";
    }
  }

  return errors;
}

function validateIssuedAt(
  issuedAt: string,
  today: string,
  errors: Record<string, string>,
) {
  if (!issuedAt.trim()) {
    errors.issuedAt = "Введите дату выдачи";
  } else if (!isDate(issuedAt)) {
    errors.issuedAt = "Некорректная дата выдачи";
  } else if (issuedAt > today) {
    errors.issuedAt = "Дата выдачи не может быть в будущем";
  }
}
