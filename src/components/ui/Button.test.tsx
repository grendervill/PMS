import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("renders its label and handles clicks", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Создать хранилище</Button>);

    fireEvent.click(screen.getByRole("button", { name: "Создать хранилище" }));

    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("does not fire when disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Сохранить
      </Button>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    expect(onClick).not.toHaveBeenCalled();
  });

  it("exposes its variant", () => {
    render(<Button variant="danger">Удалить</Button>);

    expect(screen.getByRole("button", { name: "Удалить" })).toHaveAttribute(
      "data-variant",
      "danger",
    );
  });
});
