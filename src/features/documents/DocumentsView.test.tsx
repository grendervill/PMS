import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { createDocument, emptyDocumentDraft } from "./documentModel";
import { DocumentsView } from "./DocumentsView";

const passport = createDocument(
  {
    ...emptyDocumentDraft("passport"),
    passportKind: "main",
    series: "4509",
    number: "123456",
    issuedBy: "ОВД района",
    departmentCode: "770-001",
    issuedAt: "2020-01-01",
    registrationAddress: "Москва",
  },
  "2026-10-24T10:00:00.000Z",
);
const snils = createDocument(
  {
    ...emptyDocumentDraft("snils"),
    number: "11223344595",
  },
  "2026-10-24T11:00:00.000Z",
);

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: passport.createdAt,
  updatedAt: snils.updatedAt,
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  bankCards: [],
  documents: [passport, snils],
  attachments: [],
  drafts: [],
};

function renderView() {
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <DocumentsView />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

describe("DocumentsView", () => {
  const deleteDocument = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    deleteDocument.mockClear();
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
      deleteDocument,
    });
  });

  it("shows documents and filters them", () => {
    renderView();
    expect(
      screen.getByRole("button", { name: "Паспорт РФ · 3456" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "СНИЛС · 4595" }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Поиск по документам"), {
      target: { value: "СНИЛС" },
    });
    expect(
      screen.getByRole("button", { name: "СНИЛС · 4595" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Паспорт РФ · 3456" }),
    ).not.toBeInTheDocument();
  });

  it("opens the document detail", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Паспорт РФ · 3456" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Паспорт РФ")).toBeInTheDocument();
    expect(within(dialog).getByText("Основной")).toBeInTheDocument();
    expect(within(dialog).getByText("4509 123456")).toBeInTheDocument();
  });

  it("deletes a document after confirmation", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Паспорт РФ · 3456" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));

    const confirmButtons = screen.getAllByRole("button", { name: "Удалить" });
    fireEvent.click(confirmButtons.at(-1)!);

    await waitFor(() => {
      expect(deleteDocument).toHaveBeenCalledWith(passport.id);
    });
  });

  it("opens the create editor", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Добавить документ" }));
    expect(screen.getByText("Новый документ")).toBeInTheDocument();
  });
});
