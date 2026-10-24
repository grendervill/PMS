import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { createBankCard, emptyCardDraft } from "./cardModel";
import { CardDetail } from "./CardDetail";

const card = createBankCard(
  {
    ...emptyCardDraft(),
    number: "4111111111111111",
    expiry: "12/29",
    cvv: "123",
    holder: "IVAN IVANOV",
    purpose: "Основная",
    country: "Россия",
    address: "Москва",
    postalCode: "101000",
  },
  "2026-10-24T10:00:00.000Z",
);

function renderDetail() {
  const onClose = vi.fn();
  const onEdit = vi.fn();
  const onDelete = vi.fn();
  render(
    <ToastProvider>
      <CardDetail
        card={card}
        onClose={onClose}
        onEdit={onEdit}
        onDelete={onDelete}
      />
    </ToastProvider>,
  );
  return { onClose, onEdit, onDelete };
}

describe("CardDetail", () => {
  it("hides expiry and CVV until the card is flipped", () => {
    renderDetail();

    expect(
      screen.getByRole("button", { name: "Показать обратную сторону" }),
    ).toBeInTheDocument();
    expect(screen.getAllByText("На обороте")).toHaveLength(2);
    expect(screen.getAllByText("123")).toHaveLength(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Показать обратную сторону" }),
    );

    expect(
      screen.getByRole("button", { name: "Показать лицевую сторону" }),
    ).toBeInTheDocument();
    expect(screen.queryAllByText("На обороте")).toHaveLength(0);
    expect(screen.getAllByText("123")).toHaveLength(2);
    expect(screen.getAllByText("12/29").length).toBeGreaterThanOrEqual(2);
  });

  it("exposes edit and delete actions", () => {
    const { onEdit, onDelete } = renderDetail();
    fireEvent.click(screen.getByRole("button", { name: "Редактировать" }));
    fireEvent.click(screen.getByRole("button", { name: "Удалить" }));
    expect(onEdit).toHaveBeenCalled();
    expect(onDelete).toHaveBeenCalled();
  });

  it("shows optional card fields", () => {
    renderDetail();
    expect(screen.getByText("Россия")).toBeInTheDocument();
    expect(screen.getByText("Москва")).toBeInTheDocument();
    expect(screen.getByText("101000")).toBeInTheDocument();
  });

  it("offers copy actions for every visible card field", () => {
    renderDetail();
    for (const label of [
      "Копировать номер",
      "Копировать владельца",
      "Копировать назначение",
      "Копировать страну",
      "Копировать адрес",
      "Копировать индекс",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.queryByRole("button", { name: "Копировать срок" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Копировать CVV" }),
    ).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Показать обратную сторону" }),
    );
    expect(
      screen.getByRole("button", { name: "Копировать срок" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Копировать CVV" }),
    ).toBeInTheDocument();
  });
});
