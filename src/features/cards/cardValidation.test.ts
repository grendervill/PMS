import { describe, expect, it } from "vitest";
import {
  cardLabel,
  createBankCard,
  emptyCardDraft,
  DEFAULT_CARD_PRESET,
} from "./cardModel";
import {
  digitsOnly,
  formatCardNumber,
  formatExpiryInput,
  maskCardNumber,
  normalizeHolder,
  validateCardDraft,
} from "./cardValidation";

describe("card formatting", () => {
  it("keeps only digits and groups the number by four", () => {
    expect(digitsOnly("4111-1111 abc 1111")).toBe("411111111111");
    expect(formatCardNumber("4111 1111 1111 1111")).toBe("4111 1111 1111 1111");
    expect(formatCardNumber("5555555555554444")).toBe("5555 5555 5555 4444");
  });

  it("masks all but the last four digits", () => {
    expect(maskCardNumber("4111111111111111")).toBe("•••• •••• •••• 1111");
    expect(maskCardNumber("4111")).toBe("•••• •••• •••• 4111");
  });

  it("formats expiry as MM/YY while typing", () => {
    expect(formatExpiryInput("1")).toBe("1");
    expect(formatExpiryInput("12")).toBe("12");
    expect(formatExpiryInput("123")).toBe("12/3");
    expect(formatExpiryInput("1229")).toBe("12/29");
    expect(formatExpiryInput("12/29")).toBe("12/29");
  });

  it("uppercases and keeps only Latin holder characters", () => {
    expect(normalizeHolder("ivan petrov-ivanov")).toBe("IVAN PETROV-IVANOV");
    expect(normalizeHolder("иван иванов")).toBe("");
  });
});

describe("card validation", () => {
  it("requires number, expiry, CVV and holder", () => {
    expect(validateCardDraft(emptyCardDraft())).toEqual({
      number: "Введите номер карты",
      expiry: "Введите срок в формате ММ/ГГ",
      cvv: "Введите CVV",
      holder: "Введите имя и фамилию латиницей",
    });
  });

  it("rejects a short number, bad month, short CVV and one-word holder", () => {
    const errors = validateCardDraft({
      ...emptyCardDraft(),
      number: "4111",
      expiry: "13/29",
      cvv: "12",
      holder: "IVAN",
    });
    expect(errors.number).toBe("Номер карты должен содержать 13–19 цифр");
    expect(errors.expiry).toBe("Срок должен быть в формате ММ/ГГ");
    expect(errors.cvv).toBe("CVV должен содержать 3–4 цифры");
    expect(errors.holder).toBe("Введите имя и фамилию латиницей");
  });

  it("accepts a complete card", () => {
    expect(
      validateCardDraft({
        ...emptyCardDraft(),
        number: "4111111111111111",
        expiry: "12/29",
        cvv: "123",
        holder: "IVAN IVANOV",
      }),
    ).toEqual({});
  });
});

describe("card model", () => {
  it("builds a label from purpose and last four digits", () => {
    expect(cardLabel({ number: "4111111111111111", purpose: "Основная" })).toBe(
      "Основная · 1111",
    );
    expect(cardLabel({ number: "4111111111111111", purpose: "" })).toBe(
      "Банковская карта · 1111",
    );
  });

  it("creates a card with id, schema version and timestamps", () => {
    const card = createBankCard(
      {
        ...emptyCardDraft(),
        number: " 4111111111111111 ",
        expiry: "12/29",
        cvv: "123",
        holder: "IVAN IVANOV",
      },
      "2026-10-24T10:00:00.000Z",
    );

    expect(card.number).toBe("4111111111111111");
    expect(card.schemaVersion).toBe(1);
    expect(card.imagePreset).toBe(DEFAULT_CARD_PRESET);
    expect(card.createdAt).toBe("2026-10-24T10:00:00.000Z");
    expect(card.updatedAt).toBe("2026-10-24T10:00:00.000Z");
    expect(card.id).toMatch(/[0-9a-f-]{36}/);
  });
});
