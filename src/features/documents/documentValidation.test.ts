import { describe, expect, it } from "vitest";
import {
  createDocument,
  documentChip,
  documentTitle,
  emptyDocumentDraft,
  formatDepartmentCode,
  formatDocumentNumber,
  formatSnils,
  maskDocumentNumber,
} from "./documentModel";
import { validateDocumentDraft } from "./documentValidation";

const passport = {
  ...emptyDocumentDraft("passport"),
  passportKind: "main" as const,
  fullName: "ИВАНОВ ИВАН",
  series: "4509",
  number: "123456",
  issuedBy: "ОВД района",
  departmentCode: "770-001",
  issuedAt: "2020-01-01",
  registrationAddress: "Москва, ул. Примерная, 1",
};

describe("document model", () => {
  it("names and chips document kinds", () => {
    expect(documentTitle({ kind: "passport", passportKind: "main" })).toBe(
      "Паспорт РФ",
    );
    expect(documentTitle({ kind: "passport", passportKind: "foreign" })).toBe(
      "Загранпаспорт",
    );
    expect(documentTitle({ kind: "snils", passportKind: null })).toBe("СНИЛС");
    expect(documentTitle({ kind: "driver-license", passportKind: null })).toBe(
      "Водительское удостоверение",
    );
    expect(documentChip({ kind: "passport", passportKind: "main" })).toBe(
      "Основной",
    );
    expect(documentChip({ kind: "passport", passportKind: "foreign" })).toBe(
      "Загранпаспорт",
    );
    expect(documentChip({ kind: "snils", passportKind: null })).toBe("СНИЛС");
  });

  it("formats document numbers", () => {
    expect(
      formatDocumentNumber({ kind: "passport", series: "4509", number: "123456" }),
    ).toBe("4509 123456");
    expect(
      formatDocumentNumber({
        kind: "driver-license",
        series: "9900",
        number: "123456",
      }),
    ).toBe("9900 123456");
    expect(
      formatDocumentNumber({ kind: "snils", series: "", number: "11223344595" }),
    ).toBe("112-233-445 95");
    expect(
      maskDocumentNumber({ kind: "passport", series: "4509", number: "123456" }),
    ).toBe("•••• 3456");
  });

  it("formats SNILS and department code inputs", () => {
    expect(formatSnils("11223344595")).toBe("112-233-445 95");
    expect(formatDepartmentCode("770001")).toBe("770-001");
  });

  it("creates a document with id, schema version and timestamps", () => {
    const document = createDocument(
      { ...passport, series: " 4509 ", passportKind: "main" },
      "2026-10-24T10:00:00.000Z",
    );
    expect(document.series).toBe("4509");
    expect(document.schemaVersion).toBe(1);
    expect(document.id).toMatch(/[0-9a-f-]{36}/);
    expect(document.createdAt).toBe("2026-10-24T10:00:00.000Z");
    expect(document.updatedAt).toBe("2026-10-24T10:00:00.000Z");
  });

  it("drops passportKind for non-passport documents", () => {
    const document = createDocument({
      ...emptyDocumentDraft("snils"),
      number: "11223344595",
    });
    expect(document.passportKind).toBeNull();
    expect(document.attachmentIds).toEqual([]);
  });
});

describe("document validation", () => {
  const today = "2026-10-24";

  it("validates a main passport", () => {
    expect(validateDocumentDraft(passport, today)).toEqual({});
    const errors = validateDocumentDraft(
      { ...emptyDocumentDraft("passport"), passportKind: "main" },
      today,
    );
    expect(errors.series).toBe("Введите серию паспорта");
    expect(errors.number).toBe("Введите номер паспорта");
    expect(errors.issuedBy).toBe("Введите, кем выдан документ");
    expect(errors.departmentCode).toBe("Введите код подразделения");
    expect(errors.issuedAt).toBe("Введите дату выдачи");
    expect(errors.registrationAddress).toBe("Введите адрес прописки");
  });

  it("validates a foreign passport with 2/7 digits", () => {
    expect(
      validateDocumentDraft(
        {
          ...emptyDocumentDraft("passport"),
          passportKind: "foreign",
          series: "12",
          number: "1234567",
          issuedBy: "МВД",
          issuedAt: "2024-01-01",
        },
        today,
      ),
    ).toEqual({});

    const errors = validateDocumentDraft(
      {
        ...emptyDocumentDraft("passport"),
        passportKind: "foreign",
        series: "4509",
        number: "123456",
        issuedBy: "МВД",
        issuedAt: "2024-01-01",
      },
      today,
    );
    expect(errors.series).toBe("Серия загранпаспорта должна содержать 2 цифры");
    expect(errors.number).toBe("Номер загранпаспорта должен содержать 7 цифр");
  });

  it("validates SNILS", () => {
    expect(
      validateDocumentDraft(
        { ...emptyDocumentDraft("snils"), number: "11223344595" },
        today,
      ),
    ).toEqual({});
    expect(
      validateDocumentDraft(
        { ...emptyDocumentDraft("snils"), number: "1122334459" },
        today,
      ).number,
    ).toBe("СНИЛС должен содержать 11 цифр");
  });

  it("validates a driver's license and date order", () => {
    const driver = {
      ...emptyDocumentDraft("driver-license"),
      fullName: "ИВАНОВ ИВАН",
      series: "9900",
      number: "123456",
      issuedBy: "Госавтоинспекция",
      issuedAt: "2020-01-01",
      validUntil: "2030-01-01",
      categories: "B, M",
    };
    expect(validateDocumentDraft(driver, today)).toEqual({});
    expect(
      validateDocumentDraft(
        { ...driver, validUntil: "2019-01-01" },
        today,
      ).validUntil,
    ).toBe("Срок действия не может быть раньше даты выдачи");
    expect(
      validateDocumentDraft(
        { ...driver, issuedAt: "2027-01-01" },
        today,
      ).issuedAt,
    ).toBe("Дата выдачи не может быть в будущем");
    expect(
      validateDocumentDraft({ ...driver, fullName: "ИВАН" }, today).fullName,
    ).toBe("Введите имя и фамилию");
  });
});
