import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConflictDialog } from "./ConflictDialog";

describe("ConflictDialog", () => {
  it("shows both versions and does not decide by itself", () => {
    const onKeepMine = vi.fn();
    const onTakeTheirs = vi.fn();
    const onLater = vi.fn();
    render(
      <ConflictDialog
        open
        remoteVersion={7}
        onKeepMine={onKeepMine}
        onTakeTheirs={onTakeTheirs}
        onLater={onLater}
      />,
    );

    expect(screen.getByText(/другом устройстве/i)).toBeInTheDocument();
    expect(screen.getByText(/версия 7/)).toBeInTheDocument();
    expect(onKeepMine).not.toHaveBeenCalled();
    expect(onTakeTheirs).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Оставить моё/ }));
    expect(onKeepMine).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: /Взять серверное/ }));
    expect(onTakeTheirs).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: /Позже/ }));
    expect(onLater).toHaveBeenCalledOnce();
  });

  it("promises a backup before taking the server version", () => {
    render(
      <ConflictDialog
        open
        remoteVersion={2}
        onKeepMine={vi.fn()}
        onTakeTheirs={vi.fn()}
        onLater={vi.fn()}
      />,
    );
    expect(screen.getByText(/сохранится в/i)).toBeInTheDocument();
  });
});
