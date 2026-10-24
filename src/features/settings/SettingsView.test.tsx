import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { vaultApi } from "@/features/vault/api";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import { defaultVaultSettings, type VaultData } from "@/features/vault/types";
import { SettingsView } from "./SettingsView";
import {
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";

vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn().mockResolvedValue(undefined),
  disable: vi.fn().mockResolvedValue(undefined),
  isEnabled: vi.fn().mockResolvedValue(false),
}));

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
  bankCards: [],
  documents: [],
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
    vi.spyOn(vaultApi, "setAppIcon").mockResolvedValue(undefined);
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

  it("selects and persists an application icon", async () => {
    renderSettings();

    const green = screen.getByRole("button", { name: "Зелёный замок" });
    expect(green).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Синий щит" }));

    await waitFor(() => {
      expect(localStorage.getItem("pms.appIcon")).toBe("blue-shield");
    });
    expect(vaultApi.setAppIcon).toHaveBeenCalledWith("blue-shield");
    expect(screen.getByRole("button", { name: "Синий щит" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("persists the selected auto-lock duration", async () => {
    const save = vi.spyOn(useVaultStore.getState(), "save").mockResolvedValue(true);
    renderSettings();

    const select = screen.getByLabelText("Автоблокировка при бездействии");
    fireEvent.change(select, { target: { value: "10s" } });

    await waitFor(() => {
      expect(save).toHaveBeenCalled();
    });
    expect(save.mock.calls[0]?.[0].settings.autoLock).toBe("10s");
  });

  it("persists the selected card layout", async () => {
    const save = vi.spyOn(useVaultStore.getState(), "save").mockResolvedValue(true);
    renderSettings();

    fireEvent.click(screen.getByRole("button", { name: /Компактно/ }));

    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]?.[0].settings.cardLayout).toBe("compact");
  });

  it("toggles autostart", async () => {
    renderSettings();
    const checkbox = await screen.findByRole("checkbox", {
      name: "Запускать автоматически при входе в систему",
    });
    await waitFor(() => expect(checkbox).not.toBeDisabled());
    fireEvent.click(checkbox);
    await waitFor(() => expect(enableAutostart).toHaveBeenCalled());
    expect(isAutostartEnabled).toHaveBeenCalled();
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
