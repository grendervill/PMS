import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import {
  defaultVaultSettings,
  type VaultData,
} from "@/features/vault/types";
import { createDocument, emptyDocumentDraft } from "./documentModel";
import { DocumentDetail } from "./DocumentDetail";

const passport = createDocument(
  {
    ...emptyDocumentDraft("passport"),
    passportKind: "main",
    fullName: "ИВАНОВ ИВАН",
    series: "4509",
    number: "123456",
    issuedBy: "ОВД района",
    departmentCode: "770-001",
    issuedAt: "2020-01-01",
    registrationAddress: "Москва",
  },
  "2026-10-24T10:00:00.000Z",
);

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: passport.createdAt,
  updatedAt: passport.updatedAt,
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  bankCards: [],
  documents: [passport],
  attachments: [],
  drafts: [],
};

function renderDetail(document = passport) {
  useVaultStore.setState({
    ...initialVaultState,
    status: "unlocked",
    sessionId: "session-1",
    vaultPath: "/vault",
    data: { ...data, documents: [document] },
  });
  const onClose = vi.fn();
  const onEdit = vi.fn();
  const onDelete = vi.fn();
  render(
    <ToastProvider>
      <DocumentDetail
        document={document}
        onClose={onClose}
        onEdit={onEdit}
        onDelete={onDelete}
      />
    </ToastProvider>,
  );
  return { onClose, onEdit, onDelete };
}

describe("DocumentDetail", () => {
  it("shows the passport chip and fields", () => {
    renderDetail();
    expect(screen.getByText("Паспорт РФ")).toBeInTheDocument();
    expect(screen.getByText("Основной")).toBeInTheDocument();
    expect(screen.getByText("4509 123456")).toBeInTheDocument();
    expect(screen.getByText("ОВД района")).toBeInTheDocument();
    expect(screen.getByText("770-001")).toBeInTheDocument();
    expect(screen.getByText("01.01.2020")).toBeInTheDocument();
  });

  it("offers copy actions for every visible document field", () => {
    renderDetail();
    for (const label of [
      "Копировать ФИО",
      "Копировать номер",
      "Копировать, кем выдан",
      "Копировать код подразделения",
      "Копировать дату выдачи",
      "Копировать адрес",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("offers copy actions for driver's license fields", () => {
    const driver = createDocument({
      ...emptyDocumentDraft("driver-license"),
      fullName: "ИВАНОВ ИВАН",
      series: "9900",
      number: "123456",
      issuedBy: "Госавтоинспекция",
      issuedAt: "2020-01-01",
      validUntil: "2030-01-01",
      categories: "B, M",
      birthDate: "1990-01-01",
      birthPlace: "Москва",
      registrationAddress: "Москва",
    });
    renderDetail(driver);
    for (const label of [
      "Копировать срок действия",
      "Копировать категории",
      "Копировать дату рождения",
      "Копировать место рождения",
      "Копировать адрес",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("exposes edit and delete actions", () => {
    const { onEdit, onDelete } = renderDetail();
    fireEvent.click(screen.getByRole("button", { name: "Редактировать" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));
    expect(onEdit).toHaveBeenCalled();
    expect(onDelete).toHaveBeenCalled();
  });
});
