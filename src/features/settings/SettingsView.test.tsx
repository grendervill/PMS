import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { vaultApi } from "@/features/vault/api";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { SettingsView } from "./SettingsView";

const data: VaultData = {
  schemaVersion: 1,
  id: "vault-1",
  name: "Личное",
  createdAt: "2026-10-20T10:00:00.000Z",
  updatedAt: "2026-10-20T10:00:00.000Z",
  settings: defaultVaultSettings(),
  entries: [],
  sshKeys: [],
  attachments: [],
  drafts: [],
};

function renderSettings() {
  return render(
    <ToastProvider>
      <SettingsView />
    </ToastProvider>,
  );
}

describe("SettingsView biometrics", () => {
  beforeEach(() => {
    localStorage.clear();
    useVaultStore.setState({
      ...initialVaultState,
      status: "unlocked",
      sessionId: "session-1",
      data,
    });
    vi.spyOn(vaultApi, "biometricStatus").mockResolvedValue({ available: false });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("disables biometrics when the device has no sensor", async () => {
    renderSettings();

    await waitFor(() => {
      expect(screen.getByText(/нет доступного Touch ID/)).toBeInTheDocument();
    });
    expect(
      screen.getByRole("switch", {
        name: "Всегда разблокировать при помощи биометрии",
      }),
    ).toBeDisabled();
  });

  it("enrolls biometrics when available and the switch is enabled", async () => {
    vi.mocked(vaultApi.biometricStatus).mockResolvedValue({ available: true });
    vi.spyOn(vaultApi, "biometricEnroll").mockResolvedValue(undefined);
    renderSettings();

    const toggle = await screen.findByRole("switch", {
      name: "Всегда разблокировать при помощи биометрии",
    });
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(localStorage.getItem("pms.biometric.vault-1")).toBe("1");
    });
    expect(vaultApi.biometricEnroll).toHaveBeenCalledWith("session-1");
  });
});
