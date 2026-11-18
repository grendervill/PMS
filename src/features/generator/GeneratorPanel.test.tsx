import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { vaultApi } from "@/features/vault/api";
import { GENERATOR_HISTORY_KEY } from "./history";
import { GeneratorPanel } from "./GeneratorPanel";

vi.mock("@/features/vault/api", () => ({
  vaultApi: {
    generatePassword: vi.fn(),
    generateLogin: vi.fn(),
    generatePassphrase: vi.fn(),
    generatorEntropy: vi.fn(),
  },
}));

vi.mock("@/features/generator/strength", () => ({
  passwordStrength: vi.fn().mockResolvedValue({ score: 4, label: "Очень надёжный" }),
}));

const generateMock = vi.mocked(vaultApi.generatePassword);
const generateLoginMock = vi.mocked(vaultApi.generateLogin);
/** The passphrase is rendered word by word (with <wbr/>), so match on textContent. */
const PASSPHRASE = "jitterbug-wiping-stupor-litigate-taper";

const generatePassphraseMock = vi.mocked(vaultApi.generatePassphrase);
const entropyMock = vi.mocked(vaultApi.generatorEntropy);

function seedHistory(values: { value: string; mode: string }[]) {
  globalThis.sessionStorage.setItem(
    GENERATOR_HISTORY_KEY,
    JSON.stringify(values.map((entry, index) => ({ ...entry, at: index }))),
  );
}

function blockSessionStorage() {
  const spy = vi.spyOn(globalThis, "sessionStorage", "get");
  spy.mockImplementation(() => {
    throw new DOMException("blocked", "SecurityError");
  });
  return spy;
}

describe("GeneratorPanel", () => {
  beforeEach(() => {
    globalThis.sessionStorage.clear();
    generateMock.mockReset();
    generateLoginMock.mockReset();
    generatePassphraseMock.mockReset();
    entropyMock.mockReset();
    generateMock.mockResolvedValue("Abc123!@#xyz");
    generateLoginMock.mockResolvedValue("Abc123!@#xyz");
    generatePassphraseMock.mockResolvedValue(PASSPHRASE);
    entropyMock.mockImplementation(async (input) => (input.mode === "passphrase" ? 77.55 : 104.2));
  });

  afterEach(() => {
    vi.restoreAllMocks();
    globalThis.sessionStorage.clear();
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

  it("generates a passphrase with the default options", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="passphrase" />
      </ToastProvider>,
    );

    expect(await screen.findByTestId("generated-value")).toHaveTextContent(PASSPHRASE);
    expect(generatePassphraseMock).toHaveBeenCalledWith({
      words: 5,
      separator: "-",
      capitalize: false,
      includeNumber: false,
    });
  });

  it("passes the passphrase controls to generate_passphrase", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="passphrase" />
      </ToastProvider>,
    );

    expect(await screen.findByTestId("generated-value")).toHaveTextContent(PASSPHRASE);

    fireEvent.change(screen.getByLabelText("Количество слов"), { target: { value: "8" } });
    fireEvent.change(screen.getByLabelText("Разделитель"), { target: { value: "." } });
    fireEvent.click(screen.getByRole("switch", { name: "Заглавные буквы" }));
    fireEvent.click(screen.getByRole("switch", { name: "Добавить цифру" }));

    generatePassphraseMock.mockResolvedValueOnce("correct-horse-battery-staple-anchor");
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generatePassphraseMock).toHaveBeenLastCalledWith({
        words: 8,
        separator: ".",
        capitalize: true,
        includeNumber: true,
      });
    });
    expect(await screen.findByText("correct-horse-battery-staple-anchor")).toBeInTheDocument();
  });

  it("supports an empty separator", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="passphrase" />
      </ToastProvider>,
    );

    expect(await screen.findByTestId("generated-value")).toHaveTextContent(PASSPHRASE);
    fireEvent.change(screen.getByLabelText("Разделитель"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generatePassphraseMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ separator: "" }),
      );
    });
  });

  it("sends the minimum digit requirement to generate_password", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    fireEvent.change(screen.getByLabelText("Минимум цифр"), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generateMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ digits: true, minDigits: 3, minSymbols: 0 }),
      );
    });
  });

  it("drops the minimum when the matching charset is disabled", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    fireEvent.change(screen.getByLabelText("Минимум спецсимволов"), {
      target: { value: "2" },
    });
    fireEvent.click(screen.getByRole("switch", { name: "Символы" }));
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generateMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ symbols: false, minSymbols: 0 }),
      );
    });
  });

  it("shows the entropy estimate from generator_entropy", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="passphrase" />
      </ToastProvider>,
    );

    expect(await screen.findByText("~78 бит")).toBeInTheDocument();
    expect(entropyMock).toHaveBeenCalledWith({
      mode: "passphrase",
      options: { words: 5, separator: "-", capitalize: false, includeNumber: false },
    });
  });

  it("keeps rendering when the entropy call fails", async () => {
    entropyMock.mockRejectedValue(new Error("boom"));

    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    expect(await screen.findByText("Abc123!@#xyz")).toBeInTheDocument();
    expect(screen.queryByText(/бит/)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("sends the excluded characters to the backend", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    fireEvent.change(screen.getByLabelText("Исключить символы"), {
      target: { value: "<>\"'" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generateMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ excludeChars: "<>\"'" }),
      );
    });
  });

  it("limits the excluded characters field to 64 characters", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="login" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    const field = screen.getByLabelText("Исключить символы");
    expect(field).toHaveAttribute("maxlength", "64");

    // Clearing the field means "nothing excluded", not an empty string.
    fireEvent.change(field, { target: { value: "<>" } });
    fireEvent.change(field, { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(generateLoginMock).toHaveBeenLastCalledWith(
        expect.objectContaining({ excludeChars: null }),
      );
    });
  });

  it("applies length presets and regenerates immediately", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    fireEvent.click(screen.getByRole("button", { name: "Длина 32" }));

    await waitFor(() => {
      expect(generateMock).toHaveBeenLastCalledWith(expect.objectContaining({ length: 32 }));
    });
    expect(screen.getByRole("button", { name: "Длина 32" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByLabelText("Длина")).toHaveValue(32);
  });

  it("offers the wider preset list for logins", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="login" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");

    expect(screen.getByRole("button", { name: "Длина 24" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Длина 48" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Длина 20" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Длина 64" }));

    await waitFor(() => {
      expect(generateLoginMock).toHaveBeenLastCalledWith(expect.objectContaining({ length: 64 }));
    });
  });

  it("records generated values in the session history", async () => {
    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    expect(await screen.findByTestId("generator-history")).toBeInTheDocument();
    expect(screen.getByTitle("Abc123!@#xyz")).toBeInTheDocument();

    generateMock.mockResolvedValueOnce("Zyx987$%^abc");
    fireEvent.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() => {
      expect(screen.getByTitle("Zyx987$%^abc")).toBeInTheDocument();
    });
    expect(globalThis.sessionStorage.getItem(GENERATOR_HISTORY_KEY)).toContain("Zyx987$%^abc");
  });

  it("restores a value from the history", async () => {
    seedHistory([{ value: "Seeded-Value-For-Restore", mode: "password" }]);

    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");
    fireEvent.click(screen.getByRole("button", { name: "Вернуть значение 2" }));

    expect(screen.getByTestId("generated-value")).toHaveTextContent("Seeded-Value-For-Restore");
  });

  it("hands a restored value to the caller", async () => {
    seedHistory([{ value: "Restored-Secret-Value", mode: "login" }]);
    const onUse = vi.fn();

    render(
      <ToastProvider>
        <GeneratorPanel mode="login" onUse={onUse} />
      </ToastProvider>,
    );

    await screen.findByText("Abc123!@#xyz");
    fireEvent.click(screen.getByRole("button", { name: "Вернуть значение 2" }));

    expect(onUse).toHaveBeenCalledWith("Restored-Secret-Value");
  });

  it("clears the history on demand", async () => {
    seedHistory([
      { value: "First-Hidden-Value", mode: "password" },
      { value: "Second-Hidden-Value", mode: "password" },
    ]);

    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    await screen.findByTestId("generator-history");
    fireEvent.click(screen.getByRole("button", { name: "Очистить историю" }));

    expect(screen.queryByTestId("generator-history")).not.toBeInTheDocument();
    expect(globalThis.sessionStorage.getItem(GENERATOR_HISTORY_KEY)).toBeNull();
  });

  it("explains when the exclusion list empties a whole set", async () => {
    generateLoginMock.mockRejectedValueOnce(
      Object.assign(new Error("empty charset"), { code: "empty_charset" }),
    );

    render(
      <ToastProvider>
        <GeneratorPanel mode="login" />
      </ToastProvider>,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Список исключений убирает целый набор символов",
    );
  });

  it("keeps working when sessionStorage is blocked", async () => {
    const spy = blockSessionStorage();

    render(
      <ToastProvider>
        <GeneratorPanel mode="password" />
      </ToastProvider>,
    );

    expect(await screen.findByText("Abc123!@#xyz")).toBeInTheDocument();
    // Nothing can be persisted, but the in-memory list still works this session.
    expect(screen.getByTestId("generator-history")).toBeInTheDocument();
    expect(screen.getByTitle("Abc123!@#xyz")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    spy.mockRestore();
  });
});
