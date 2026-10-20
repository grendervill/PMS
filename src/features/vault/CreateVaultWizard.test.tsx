import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initialVaultState, useVaultStore } from "./store";
import { CreateVaultWizard } from "./CreateVaultWizard";

describe("CreateVaultWizard", () => {
  beforeEach(() => {
    useVaultStore.setState(initialVaultState);
  });

  it("shows folder and name errors before moving on", () => {
    render(<CreateVaultWizard onClose={vi.fn()} />);

    expect(screen.getByText("Где будет жить сейф?")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Далее/ }));

    expect(screen.getByText("Выберите папку для хранилища")).toBeInTheDocument();
    expect(screen.getByText("Введите название хранилища")).toBeInTheDocument();
  });
});
