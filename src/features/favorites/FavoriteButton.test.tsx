import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import {
  defaultVaultSettings,
  type VaultData,
} from "@/features/vault/types";
import { FavoriteButton } from "./FavoriteButton";

function vault(favorites: VaultData["favorites"] = []): VaultData {
  return {
    schemaVersion: 1,
    id: "vault-1",
    name: "Личное",
    createdAt: "2026-10-24T10:00:00.000Z",
    updatedAt: "2026-10-24T10:00:00.000Z",
    settings: defaultVaultSettings(),
    entries: [],
    sshKeys: [],
    attachments: [],
    drafts: [],
    bankCards: [],
    documents: [],
    favorites,
  };
}

describe("FavoriteButton", () => {
  it("adds an item to favorites and shows a toast", async () => {
    const toggleFavorite = vi.fn().mockResolvedValue(true);
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      data: vault(),
      toggleFavorite,
    });

    render(
      <ToastProvider>
        <FavoriteButton kind="entry" id="entry-1" />
      </ToastProvider>,
    );

    const button = screen.getByRole("button", {
      name: "Добавить в избранное",
    });
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);

    await waitFor(() => {
      expect(toggleFavorite).toHaveBeenCalledWith("entry", "entry-1");
    });
    await waitFor(() => {
      expect(screen.getByText("Добавлено в избранное")).toBeInTheDocument();
    });
  });

  it("shows the active state and removes an item", async () => {
    const toggleFavorite = vi.fn().mockResolvedValue(true);
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      data: vault([{ kind: "entry", id: "entry-1" }]),
      toggleFavorite,
    });

    render(
      <ToastProvider>
        <FavoriteButton kind="entry" id="entry-1" />
      </ToastProvider>,
    );

    const button = screen.getByRole("button", {
      name: "Убрать из избранного",
    });
    expect(button).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(button);

    await waitFor(() => {
      expect(toggleFavorite).toHaveBeenCalledWith("entry", "entry-1");
    });
    await waitFor(() => {
      expect(screen.getByText("Убрано из избранного")).toBeInTheDocument();
    });
  });
});
