import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { createBankCard, emptyCardDraft } from "./cardModel";
import { CardEditor } from "./CardEditor";

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

function renderEditor(card = null as ReturnType<typeof createBankCard> | null) {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(
    <ToastProvider>
      <ConfirmProvider>
        <CardEditor card={card} onClose={onClose} onSaved={onSaved} />
      </ConfirmProvider>
    </ToastProvider>,
  );
  return { onClose, onSaved };
}

describe("CardEditor", () => {
  const addBankCard = vi.fn().mockResolvedValue(true);
  const updateBankCard = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    addBankCard.mockClear();
    updateBankCard.mockClear();
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
      addBankCard,
      updateBankCard,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("requires the four card fields", async () => {
    renderEditor();
    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    expect(await screen.findByText("Введите номер карты")).toBeInTheDocument();
    expect(screen.getByText("Введите срок в формате ММ/ГГ")).toBeInTheDocument();
    expect(screen.getByText("Введите CVV")).toBeInTheDocument();
    expect(screen.getByText("Введите имя и фамилию латиницей")).toBeInTheDocument();
    expect(addBankCard).not.toHaveBeenCalled();
  });

  it("uppercases the holder and formats the expiry", () => {
    renderEditor();
    const holder = screen.getByLabelText("Имя владельца");
    fireEvent.change(holder, { target: { value: "ivan " } });
    expect(holder).toHaveValue("IVAN ");
    fireEvent.change(holder, { target: { value: "ivan ivanov" } });
    expect(holder).toHaveValue("IVAN IVANOV");

    const expiry = screen.getByLabelText("Срок действия");
    fireEvent.change(expiry, { target: { value: "1229" } });
    expect(expiry).toHaveValue("12/29");
  });

  it("saves a new card", async () => {
    const { onClose, onSaved } = renderEditor();
    fireEvent.change(screen.getByLabelText("Номер карты"), {
      target: { value: "4111111111111111" },
    });
    fireEvent.change(screen.getByLabelText("Срок действия"), {
      target: { value: "12/29" },
    });
    fireEvent.change(screen.getByLabelText("CVV"), { target: { value: "123" } });
    fireEvent.change(screen.getByLabelText("Имя владельца"), {
      target: { value: "IVAN IVANOV" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => {
      expect(addBankCard).toHaveBeenCalledWith(
        expect.objectContaining({
          number: "4111111111111111",
          expiry: "12/29",
          cvv: "123",
          holder: "IVAN IVANOV",
        }),
      );
    });
    expect(onSaved).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("updates an existing card", async () => {
    const card = createBankCard(
      { ...emptyCardDraft(), number: "4111111111111111", expiry: "12/29", cvv: "123", holder: "IVAN IVANOV" },
      "2026-10-24T10:00:00.000Z",
    );
    renderEditor(card);
    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => {
      expect(updateBankCard).toHaveBeenCalledWith(
        card.id,
        expect.objectContaining({ number: "4111111111111111" }),
      );
    });
  });
});
