import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { EntryEditor } from "./EntryEditor";

describe("EntryEditor", () => {
  beforeEach(() => {
    useVaultStore.setState(initialVaultState);
  });

  it("requires a name before saving", () => {
    render(
      <ToastProvider>
        <EntryEditor onClose={vi.fn()} />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(screen.getByText("Введите название записи")).toBeInTheDocument();
  });
});
