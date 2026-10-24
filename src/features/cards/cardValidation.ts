import type { BankCardDraft } from "@/features/vault/types";

export function digitsOnly(value: string, max = 19): string {
  return value.replace(/\D/g, "").slice(0, max);
}

export function formatCardNumber(value: string): string {
  return digitsOnly(value).replace(/(.{4})/g, "$1 ").trim();
}

export function maskCardNumber(value: string): string {
  const last4 = digitsOnly(value).slice(-4);
  return `•••• •••• •••• ${last4}`;
}

export function formatExpiryInput(value: string): string {
  const digits = digitsOnly(value, 4);
  return digits.length > 2 ? `${digits.slice(0, 2)}/${digits.slice(2)}` : digits;
}

export function normalizeHolder(value: string): string {
  return value
    .toUpperCase()
    .replace(/[^A-Z '-]/g, "")
    .replace(/\s+/g, " ")
    .replace(/^\s+/, "");
}

export function validateCardDraft(draft: BankCardDraft): Record<string, string> {
  const errors: Record<string, string> = {};
  const number = digitsOnly(draft.number);

  if (!number) {
    errors.number = "Введите номер карты";
  } else if (number.length < 13 || number.length > 19) {
    errors.number = "Номер карты должен содержать 13–19 цифр";
  }

  if (!draft.expiry.trim()) {
    errors.expiry = "Введите срок в формате ММ/ГГ";
  } else if (!/^(0[1-9]|1[0-2])\/\d{2}$/.test(draft.expiry.trim())) {
    errors.expiry = "Срок должен быть в формате ММ/ГГ";
  }

  const cvv = draft.cvv.replace(/\D/g, "");
  if (!cvv) {
    errors.cvv = "Введите CVV";
  } else if (cvv.length < 3 || cvv.length > 4) {
    errors.cvv = "CVV должен содержать 3–4 цифры";
  }

  const holder = normalizeHolder(draft.holder).trim();
  if (!/^[A-Z][A-Z'’-]*(?: [A-Z][A-Z'’-]*)+$/.test(holder)) {
    errors.holder = "Введите имя и фамилию латиницей";
  }

  return errors;
}
