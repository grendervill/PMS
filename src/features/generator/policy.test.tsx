import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { defaultPasswordOptions } from "@/features/generator/types";
import { vaultApi } from "@/features/vault/api";
import type { PasswordPolicy } from "@/features/vault/types";
import { optionsWithPolicy, policyLengthBounds, policyNote } from "./policy";
import { GeneratorPanel } from "./GeneratorPanel";

vi.mock("@/features/vault/api", () => ({
  vaultApi: {
    generatePassword: vi.fn(),
    generateLogin: vi.fn(),
    breachCount: vi.fn(),
    generatorEntropy: vi.fn(),
  },
}));

vi.mock("@/features/generator/strength", () => ({
  passwordStrength: vi.fn().mockResolvedValue({ score: 4, label: "Очень надёжный" }),
}));

const generateMock = vi.mocked(vaultApi.generatePassword);
const entropyMock = vi.mocked(vaultApi.generatorEntropy);

const policy: PasswordPolicy = {
  minLength: 12,
  maxLength: 20,
  requireLowercase: false,
  requireUppercase: false,
  requireDigits: true,
  requireSymbols: false,
  forbiddenChars: "<>",
};

describe("optionsWithPolicy", () => {
  it("leaves manual options alone without a policy", () => {
    const options = { ...defaultPasswordOptions };
    expect(optionsWithPolicy(options, null)).toBe(options);
    expect(optionsWithPolicy(options, undefined)).toBe(options);
  });

  it("clamps the length into the site range", () => {
    expect(optionsWithPolicy({ ...defaultPasswordOptions, length: 8 }, policy).length).toBe(12);
    expect(optionsWithPolicy({ ...defaultPasswordOptions, length: 64 }, policy).length).toBe(20);
    expect(optionsWithPolicy({ ...defaultPasswordOptions, length: 16 }, policy).length).toBe(16);
  });

  it("switches the required sets on and keeps the minimums", () => {
    const options = optionsWithPolicy(
      { ...defaultPasswordOptions, digits: false },
      { ...policy, requireUppercase: true, requireSymbols: true },
    );
    expect(options.digits).toBe(true);
    expect(options.uppercase).toBe(true);
    expect(options.symbols).toBe(true);
    expect(options.minDigits).toBe(1);
    expect(options.minSymbols).toBe(1);
  });

  it("turns the banned characters into the exclusion list", () => {
    expect(optionsWithPolicy({ ...defaultPasswordOptions }, policy).excludeChars).toBe("<>");
  });

  it("keeps the generator bounds when the policy asks for nothing", () => {
    expect(policyLengthBounds(null)).toEqual({ min: 8, max: 128 });
    expect(policyLengthBounds({ ...policy, minLength: 0, maxLength: 0 })).toEqual({
      min: 8,
      max: 128,
    });
    expect(policyLengthBounds({ ...policy, minLength: 4, maxLength: 400 })).toEqual({
      min: 8,
      max: 128,
    });
    expect(policyLengthBounds({ ...policy, maxLength: 0 })).toEqual({ min: 12, max: 128 });
    expect(policyLengthBounds({ ...policy, minLength: 0 })).toEqual({ min: 8, max: 20 });
  });

  it("describes the applied rules for the dialog", () => {
    expect(policyNote(policy)).toBe(
      "Учтены правила сайта: длина 12–20 · обязательны: цифры · запрещены <>",
    );
    expect(policyNote(null)).toBeNull();
  });
});

describe("GeneratorPanel with a site policy", () => {
  beforeEach(() => {
    globalThis.sessionStorage.clear();
    generateMock.mockReset().mockResolvedValue("Abc123!@#xyz");
    entropyMock.mockReset().mockResolvedValue(104.2);
  });

  function renderPanel(extra: Partial<Parameters<typeof GeneratorPanel>[0]> = {}) {
    return render(
      <ToastProvider>
        <GeneratorPanel mode="password" policy={policy} {...extra} />
      </ToastProvider>,
    );
  }

  it("applies the policy to the controls and says so", async () => {
    renderPanel();

    await waitFor(() => expect(generateMock).toHaveBeenCalled());
    expect(screen.getByLabelText("Длина")).toHaveValue(16);
    expect(screen.getByTestId("generator-policy-note")).toHaveTextContent(
      "Учтены правила сайта: длина 12–20",
    );

    const digits = screen.getByRole("switch", { name: "Цифры" });
    expect(digits).toBeChecked();
    // A required set cannot be switched off.
    expect(digits).toBeDisabled();

    expect(screen.getByLabelText("Исключить символы")).toHaveValue("<>");
  });

  it("keeps the length inside the site range when the slider moves", async () => {
    renderPanel();

    await waitFor(() => expect(generateMock).toHaveBeenCalled());

    fireEvent.change(screen.getByLabelText("Длина ползунком"), {
      target: { value: "40" },
    });

    const length = screen.getByLabelText("Длина") as HTMLInputElement;
    expect(Number(length.value)).toBeLessThanOrEqual(20);
    expect(Number(length.value)).toBeGreaterThanOrEqual(12);

    // Presets outside the range are gone.
    expect(screen.queryByRole("button", { name: "Длина 8" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Длина 32" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Длина 12" })).toBeInTheDocument();
  });

  it("keeps the manual generator untouched without a policy", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await waitFor(() => expect(generateMock).toHaveBeenCalled());

    expect(screen.getByLabelText("Длина")).toHaveValue(16);
    expect(screen.getByRole("switch", { name: "Цифры" })).toBeEnabled();
    expect(screen.getByLabelText("Исключить символы")).toHaveValue("");
    expect(screen.queryByTestId("generator-policy-note")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Длина 8" })).toBeInTheDocument();
  });

  it("does not apply password rules to the login generator", async () => {
    const generateLogin = vi.mocked(vaultApi.generateLogin);
    generateLogin.mockReset().mockResolvedValue("alex_generated_42");
    render(
      <ToastProvider>
        <GeneratorPanel mode="login" policy={policy} />
      </ToastProvider>,
    );

    await waitFor(() => expect(generateLogin).toHaveBeenCalled());
    await waitFor(() => expect(entropyMock).toHaveBeenCalled());
    expect(screen.queryByTestId("generator-policy-note")).not.toBeInTheDocument();
  });
});
