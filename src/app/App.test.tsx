import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import { initialVaultState, useVaultStore } from "@/features/vault/store";

describe("App", () => {
  beforeEach(() => {
    localStorage.clear();
    useVaultStore.setState(initialVaultState);
  });

  it("asks where the vault lives on a fresh install", () => {
    render(<App />);
    expect(screen.getByText("Где живёт ваш сейф?")).toBeInTheDocument();
    expect(screen.getAllByText("PMS").length).toBeGreaterThan(0);
  });

  it("remembers the local choice and lands on the welcome screen", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Работать с папкой/ }));

    await waitFor(() => expect(screen.getByText("Добро пожаловать")).toBeInTheDocument());
    expect(localStorage.getItem("pms.storageMode.v1")).toBe("local");
  });

  it("opens the server profile form when the server is chosen", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Подключить сервер/ }));

    expect(screen.getByText("Подключение к серверу")).toBeInTheDocument();
  });
});
