import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StorageModeScreen } from "./StorageModeScreen";

describe("StorageModeScreen", () => {
  it("offers exactly two ways and reports the choice", () => {
    const onLocal = vi.fn();
    const onServer = vi.fn();
    render(<StorageModeScreen onLocal={onLocal} onServer={onServer} />);

    expect(screen.getByText("Где живёт ваш сейф?")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Работать с папкой/ }));
    expect(onLocal).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: /Подключить сервер/ }));
    expect(onServer).toHaveBeenCalledOnce();
  });

  it("remembers the choice on the buttons that write it", () => {
    render(<StorageModeScreen onLocal={vi.fn()} onServer={vi.fn()} />);
    expect(screen.getByText(/запомним/i)).toBeInTheDocument();
  });
});
