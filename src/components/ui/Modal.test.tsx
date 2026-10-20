import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Modal } from "./Modal";

describe("Modal", () => {
  it("shows its title and children when open", () => {
    render(
      <Modal open onOpenChange={() => undefined} title="Новая запись">
        <p>Поля записи</p>
      </Modal>,
    );

    expect(screen.getByRole("dialog", { name: "Новая запись" })).toBeInTheDocument();
    expect(screen.getByText("Поля записи")).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    render(
      <Modal open={false} onOpenChange={() => undefined} title="Новая запись">
        <p>Поля записи</p>
      </Modal>,
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("closes on Escape", () => {
    const onOpenChange = vi.fn();
    render(
      <Modal open onOpenChange={onOpenChange} title="Настройки">
        <p>Содержимое</p>
      </Modal>,
    );

    fireEvent.keyDown(document, { key: "Escape", code: "Escape" });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
