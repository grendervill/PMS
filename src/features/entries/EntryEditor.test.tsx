import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmProvider } from "@/components/ui/ConfirmProvider";
import { ToastProvider } from "@/components/ui/Toast";
import { initialVaultState, useVaultStore } from "@/features/vault/store";
import type { Entry } from "@/features/vault/types";
import { EntryEditor } from "./EntryEditor";

const policy = {
  minLength: 12,
  maxLength: 20,
  requireLowercase: true,
  requireUppercase: false,
  requireDigits: true,
  requireSymbols: false,
  forbiddenChars: "<>",
};

const entryWithPolicy: Entry = {
  id: "entry-1",
  kind: "login",
  schemaVersion: 1,
  name: "GitHub",
  login: "alex",
  password: "s3cret",
  url: "https://github.com",
  urls: ["https://github.com"],
  comment: "",
  totp: null,
  policy,
  attachmentIds: [],
  sshKeyIds: [],
  loginHistory: [],
  passwordHistory: [],
  createdAt: "2026-10-20T10:00:00.000Z",
  updatedAt: "2026-10-20T10:00:00.000Z",
};

function renderEditor(node: { onClose: () => void; entry?: Entry | null }) {
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <EntryEditor {...node} />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

describe("EntryEditor", () => {
  beforeEach(() => {
    useVaultStore.setState(initialVaultState);
  });

  it("stores the password rules a site imposes", async () => {
    const addEntry = vi.fn().mockResolvedValue(true);
    useVaultStore.setState({ ...initialVaultState, addEntry });
    renderEditor({ onClose: vi.fn() });

    fireEvent.change(screen.getByLabelText("Название"), { target: { value: "Bank" } });
    fireEvent.click(screen.getByRole("button", { name: /Правила пароля на сайте/ }));
    fireEvent.change(screen.getByLabelText("Минимальная длина"), { target: { value: "12" } });
    fireEvent.change(screen.getByLabelText("Максимальная длина"), { target: { value: "20" } });
    fireEvent.click(screen.getByLabelText("Строчные буквы"));
    fireEvent.click(screen.getByLabelText("Цифры"));
    fireEvent.change(screen.getByLabelText("Запрещённые символы"), {
      target: { value: "<>" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(addEntry).toHaveBeenCalledTimes(1);
    expect(addEntry.mock.calls[0]?.[0].policy).toEqual(policy);
  });

  it("saves nothing extra when the rules are left empty", () => {
    const addEntry = vi.fn().mockResolvedValue(true);
    useVaultStore.setState({ ...initialVaultState, addEntry });
    renderEditor({ onClose: vi.fn() });

    fireEvent.change(screen.getByLabelText("Название"), { target: { value: "Bank" } });
    fireEvent.click(screen.getByRole("button", { name: /Правила пароля на сайте/ }));
    expect(screen.getByText("не заданы")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(addEntry).toHaveBeenCalledTimes(1);
    expect(addEntry.mock.calls[0]?.[0].policy).toBeUndefined();
  });

  it("shows the rules of an existing entry and resets them", () => {
    const updateEntry = vi.fn().mockResolvedValue({ ok: true });
    useVaultStore.setState({ ...initialVaultState, updateEntry });
    renderEditor({ entry: entryWithPolicy, onClose: vi.fn() });

    expect(screen.getByLabelText("Минимальная длина")).toHaveValue(12);
    expect(screen.getByLabelText("Цифры")).toBeChecked();
    expect(screen.getByLabelText("Запрещённые символы")).toHaveValue("<>");

    fireEvent.click(screen.getByRole("button", { name: "Сбросить правила" }));
    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(updateEntry).toHaveBeenCalledTimes(1);
    expect(updateEntry.mock.calls[0]?.[1].policy).toBeUndefined();
  });

  it("refuses a minimum length above the maximum", () => {
    const updateEntry = vi.fn().mockResolvedValue({ ok: true });
    useVaultStore.setState({ ...initialVaultState, updateEntry });
    renderEditor({ entry: entryWithPolicy, onClose: vi.fn() });

    fireEvent.change(screen.getByLabelText("Минимальная длина"), { target: { value: "30" } });
    fireEvent.change(screen.getByLabelText("Максимальная длина"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(screen.getByText("Минимальная длина больше максимальной")).toBeInTheDocument();
    expect(updateEntry).not.toHaveBeenCalled();
  });

  it("requires a name before saving", () => {
    render(
      <ToastProvider>
        <ConfirmProvider>
          <EntryEditor onClose={vi.fn()} />
        </ConfirmProvider>
      </ToastProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: /Сохранить/ }));

    expect(screen.getByText("Введите название записи")).toBeInTheDocument();
  });
});
