import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { vaultApi } from "@/features/vault/api";
import { GeneratorPanel } from "./GeneratorPanel";

vi.mock("@/features/vault/api", () => ({
  vaultApi: {
    generatePassword: vi.fn(),
    generateLogin: vi.fn(),
  },
}));

vi.mock("@/features/generator/strength", () => ({
  passwordStrength: vi.fn().mockResolvedValue({ score: 4, label: "Очень надёжный" }),
}));

const generateMock = vi.mocked(vaultApi.generatePassword);
const generateLoginMock = vi.mocked(vaultApi.generateLogin);

describe("GeneratorPanel", () => {
  beforeEach(() => {
    generateMock.mockReset();
    generateLoginMock.mockReset();
    generateMock.mockResolvedValue("Abc123!@#xyz");
    generateLoginMock.mockResolvedValue("Abc123!@#xyz");
  });

  it("generates a password on open and re-generates on demand", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    expect(await screen.findByText("Abc123!@#xyz")).toBeInTheDocument();
    expect(generateMock).toHaveBeenCalledTimes(1);

    generateMock.mockResolvedValueOnce("Zyx987$%^abc");
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(screen.getByText("Zyx987$%^abc")).toBeInTheDocument();
    });
  });

  it("offers the generated value to the caller", async () => {
    const onUse = vi.fn();
    render(
      <ToastProvider>
        <GeneratorPanel mode="login" onUse={onUse} />
      </ToastProvider>,
    );

    expect(await screen.findByText("Abc123!@#xyz")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Использовать" }));

    expect(onUse).toHaveBeenCalledWith("Abc123!@#xyz");
  });
});
