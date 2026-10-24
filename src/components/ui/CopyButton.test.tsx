import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { CopyButton } from "./CopyButton";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
  readText: vi.fn(),
}));

const writeMock = vi.mocked(writeText);
const readMock = vi.mocked(readText);

function vault(clearSeconds: number): VaultData {
  return {
    schemaVersion: 1,
    id: "vault-1",
    name: "Личное",
    createdAt: "2026-10-20T10:00:00.000Z",
    updatedAt: "2026-10-20T10:00:00.000Z",
    settings: { ...defaultVaultSettings(), clipboardClearSeconds: clearSeconds },
    entries: [],
    sshKeys: [],
    attachments: [],
    drafts: [],
    bankCards: [],
    documents: [],
  };
}

describe("CopyButton", () => {
  beforeEach(() => {
    writeMock.mockReset();
    readMock.mockReset();
    writeMock.mockResolvedValue(undefined);
    readMock.mockResolvedValue("secret");
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      data: vault(5),
    });
  });

  it("uses the configured timeout and clears the copied value", async () => {
    render(
      <ToastProvider>
        <CopyButton value="secret" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Копировать" }));
    await waitFor(() => expect(writeMock).toHaveBeenCalledWith("secret"));
    await waitFor(() =>
      expect(
        screen.getByText("Буфер обмена очистится через 5 секунд."),
      ).toBeInTheDocument(),
    );

    await new Promise((resolve) => setTimeout(resolve, 5_200));
    expect(readMock).toHaveBeenCalled();
    expect(writeMock).toHaveBeenLastCalledWith("");
  }, 10_000);

  it("does not schedule clearing when the timeout is disabled", async () => {
    useVaultStore.setState({ data: vault(0) });
    render(
      <ToastProvider>
        <CopyButton value="secret" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Копировать" }));
    await waitFor(() => expect(writeMock).toHaveBeenCalledWith("secret"));
    await waitFor(() =>
      expect(screen.getByText("Автоочистка буфера обмена отключена.")).toBeInTheDocument(),
    );

    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(readMock).not.toHaveBeenCalled();
    expect(writeMock).toHaveBeenCalledTimes(1);
  });
});
