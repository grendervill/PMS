import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { createBankCard, emptyCardDraft } from "./cardModel";
import { CardsView } from "./CardsView";

const first = createBankCard(
  {
    ...emptyCardDraft(),
    number: "4111111111111111",
    expiry: "12/29",
    cvv: "123",
    holder: "IVAN IVANOV",
    purpose: "Основная",
  },
  "2026-10-24T10:00:00.000Z",
);
const second = createBankCard(
  {
    ...emptyCardDraft(),
    number: "5555555555554444",
    expiry: "09/28",
    cvv: "456",
    holder: "PETR PETROV",
    purpose: "Для подписок",
  },
  "2026-10-24T11:00:00.000Z",
);

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: "2026-10-24T10:00:00.000Z",
  updatedAt: "2026-10-24T11:00:00.000Z",
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  bankCards: [first, second],
  documents: [],
  attachments: [],
  drafts: [],
};

function renderView() {
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <CardsView />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

describe("CardsView", () => {
  const deleteBankCard = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    deleteBankCard.mockClear();
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      vaultPath: "/vault",
      data,
      deleteBankCard,
    });
  });

  it("shows cards and filters by number or holder", () => {
    renderView();
    expect(
      screen.getByRole("button", { name: "Основная · 1111" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Для подписок · 4444" }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Поиск по картам"), {
      target: { value: "4444" },
    });
    expect(
      screen.getByRole("button", { name: "Для подписок · 4444" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Основная · 1111" }),
    ).not.toBeInTheDocument();
  });

  it("opens the detail and flips the card", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Основная · 1111" }));
    expect(
      screen.getByRole("button", { name: "Показать обратную сторону" }),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "Показать обратную сторону" }),
    );
    expect(
      screen.getByRole("button", { name: "Показать лицевую сторону" }),
    ).toBeInTheDocument();
  });

  it("deletes a card after confirmation", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Основная · 1111" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));

    const confirmButtons = screen.getAllByRole("button", { name: "Удалить" });
    fireEvent.click(confirmButtons.at(-1)!);

    await waitFor(() => {
      expect(deleteBankCard).toHaveBeenCalledWith(first.id);
    });
  });

  it("opens the create editor", () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "Добавить карту" }));
    expect(screen.getByText("Новая карта")).toBeInTheDocument();
  });
});
