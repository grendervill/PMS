import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { DocumentEditor } from "./DocumentEditor";

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: "2026-10-24T10:00:00.000Z",
  updatedAt: "2026-10-24T10:00:00.000Z",
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  bankCards: [],
  documents: [],
  attachments: [],
  drafts: [],
};

function renderEditor() {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(
    <ToastProvider>
      <ConfirmProvider>
        <DocumentEditor document={null} onClose={onClose} onSaved={onSaved} />
      </ConfirmProvider>
    </ToastProvider>,
  );
  return { onClose, onSaved };
}

describe("DocumentEditor", () => {
  const addDocument = vi.fn().mockResolvedValue(true);
  const updateDocument = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    addDocument.mockClear();
    updateDocument.mockClear();
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
      addDocument,
      updateDocument,
    });
  });

  it("switches fields with the document type", () => {
    renderEditor();
    expect(screen.getByLabelText("Серия")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^СНИЛС/ }));
    expect(screen.getByLabelText("Номер СНИЛС")).toBeInTheDocument();
    expect(screen.queryByLabelText("Кем выдан")).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /^Водительское удостоверение/ }),
    );
    expect(screen.getByLabelText("Категории")).toBeInTheDocument();
  });

  it("requires passport fields", async () => {
    renderEditor();
    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    expect(await screen.findByText("Введите серию паспорта")).toBeInTheDocument();
    expect(screen.getByText("Введите номер паспорта")).toBeInTheDocument();
    expect(addDocument).not.toHaveBeenCalled();
  });

  it("formats SNILS and saves it", async () => {
    renderEditor();
    fireEvent.click(screen.getByRole("button", { name: /^СНИЛС/ }));
    const number = screen.getByLabelText("Номер СНИЛС");
    fireEvent.change(number, { target: { value: "11223344595" } });
    expect(number).toHaveValue("112-233-445 95");
    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => {
      expect(addDocument).toHaveBeenCalledWith(
        expect.objectContaining({ kind: "snils", number: "11223344595" }),
      );
    });
  });
});
